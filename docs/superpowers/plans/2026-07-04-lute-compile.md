# Lute Compiler (lute-compile) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Lower a *checked* `.lute` visual-novel document into a flat, typed JSON command-record artifact, surfaced as a `lute compile` CLI subcommand.

**Architecture:** A new `lute-compile` crate runs the spec's 7-pass pipeline (design spec `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`, §5): gate on a clean `check()` (D6) → AST normalization of `::use` components and choice `persist` (D8) → `@ref`/`@fn(args)`/`$` inline-CEL expansion (D4) → direct lowering + branch/match flattening over symbolic labels → CFG-aware stage resolution with fork/join and inline timeline scheduling (D9) → dense `addr` + `lineId`/`voiceKey` assignment → deterministic `serde_json` serialization. It reuses `lute-check`'s validators, injection reducer, timeline math, and folded state schema; `lute-cli` gains a thin `compile` subcommand.

**Tech Stack:** Rust (stable), serde/serde_json, cargo-insta; reuses lute-syntax/lute-cel/lute-check/lute-manifest.

## Global Constraints

- **Worktree:** ALL work happens in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-compile`). Every command below runs from that directory. The harness resolves relative paths to the MAIN tree — always `cd` to the absolute worktree path first.
- **Toolchain:** rustup stable at `~/.cargo/bin`. Every shell: `export PATH="$HOME/.cargo/bin:$PATH"`. The shared target dir comes from `/Users/journey/Workspace/lute/.cargo/config.toml` (cargo ancestor lookup — do NOT create a per-worktree `.cargo/`).
- **Per-task hygiene (before every commit):** `cargo fmt -p <crate>` then `cargo clippy -p <crate> --all-targets -- -D warnings` then `cargo test -p <crate>` for the crate(s) the task touched. NO project-wide fmt/lint/test mid-plan; the full-workspace gate is Task 15 only.
- **TDD:** every step writes the failing test first, runs it to see the failure, then the minimal implementation.
- **Determinism:** `BTreeMap`/`BTreeSet` only — `HashMap`/`HashSet` are PROHIBITED in `lute-compile`. Fixed struct field order; JSON via `serde_json::to_string_pretty` + one trailing `\n`; same input ⇒ byte-identical output.
- **Never panic:** no `unwrap`/`expect`/`panic!`/`unreachable!` on document-derived data in `lute-compile` library code (tests may). Failures degrade to `Diagnostic`s (`E-COMPILE-EXPAND`, `E-COMPILE-COMPONENT`, `E-COMPILE-INTERNAL`).
- **Crate DAG:** `lute-syntax`/`lute-cel`/`lute-manifest`/`lute-check` ← `lute-compile` ← `lute-cli` (acyclic, D7). `lute-compile` NEVER depends on `lute-cli` or `lute-lsp`. New public types live in `lute-compile`.
- **`lute-check` widening is ADDITIVE only** (new diagnostic, new pub fn/struct; no signature changes to existing pub items). After touching `lute-check`, run `cargo test -p lute-check -p lute-lsp -p lute-cli` and keep `crates/lute-lsp/tests/divergence.rs` green.
- **No tree-sitter / capabilityVersion impact:** never touch `crates/lute-manifest/assets/**`; `cargo test -p lute-manifest --test tree_sitter_stamp` must stay green (verified in Task 15).
- **3-id model (spec §4.2):** `addr` + `lineId` + `voiceKey` ONLY. No `textUnitId`; no serialized standalone `code`/`shot` fields (`LineCmd.code` is `#[serde(skip)]`).
- **Snapshots:** `insta = { version 1, features ["yaml"] }` is already a workspace dependency; `cargo-insta 1.48.0` is installed at `~/.cargo/bin/cargo-insta` (verified). Accept new snapshots with `cargo insta accept` AFTER eyeballing the `.snap.new` content.
- **Commits:** one per task, conventional message as given in the task's final step. `git add` exactly the files listed.

## Spec-Gap Notes (plan comments — smallest-possible resolutions, NO new scope)

These are the only points where the design spec is silent; each resolution below is the minimal choice consistent with the spec's decisions. Do not widen them.

1. **Plugin directives** (e.g. showcase's `::serve`, `::minigame`): §4.4 lists core record kinds only, yet the §8 E2E goldens (`showcase/`) contain plugin directives. Resolution: a non-core directive lowers to a generic passthrough record `{"kind":"plugin","tag":"<tag>","fields":{…}}` with fields typed via the directive's manifest `AttrDecl` (Task 7). No per-plugin schema is invented.
2. **Convergence at end-of-shot** (a `<branch>`/`<match>` as the last node): the converge label has no following record. Resolution: bind it to the one-past-end addr of the shot (next `+100` slot); the VM treats a PC past the last record as end-of-episode fall-through (Task 11).
3. **Envelope `meta.title`:** `lute_check::meta::TypedMeta` does not lift the frontmatter `title:`. Resolution: read it from `doc.meta.raw_yaml` in `lute-compile` (Task 12) instead of widening `lute-check` further.
4. **Injected-record placement:** §4.5 pins order only for `::auto` (authored sprite first, injected preload second). Resolution: for `::auto` the authored record precedes its injections; for every other node (`:line` posReset, `::bg` hides) injections precede the authored record (Task 9; pinned by tests).
5. **`wait` for `::bg`:** the core manifest declares no `wait` attr on `bg`, but §4.4 says background `wait` defaults true. Resolution: `effective_wait` = author attr → manifest `AttrDecl.default` → builtin fallback `{bg: true, video: true, else none}` (Task 7). `::camera` keeps its manifest default `false` (§11 open question).
6. **Expander cycles:** the checker does not prove def-body acyclicity, so the D4 cycle guard emits its own `E-COMPILE-EXPAND` Error diagnostic and compile aborts (Task 5).
7. **Stage join `Unknown`** (§7.3): `lute_check::StageState` has no Unknown variant. Resolution: a slot that differs across arms (or is present in only some) is DROPPED from `on_stage`/`dirty` at the join — a following plain line makes no pose assumption (no false posReset) and a following `::auto` is a fresh show (anchor + preload). Documented v1-conservative (Task 9).
8. **Duplicate injection-conflict warnings:** `compile()`'s own CFG walk re-derives `W-INJECT-CONFLICT`s that `check()` already reported. Resolution: `compile()` clears the walk's `StageState.diags` — `check()` is the reporting surface (Task 12).
9. **`::use` inside a `<timeline>` clip:** a `ClipNode` is `Directive|Set`, so normalization (which rewrites `Node`s, not clip nodes) cannot inline-expand a `::use` clip. This MUST NOT be dropped silently — an unexpanded `::use` reaching lowering is a **fail-loud `E-COMPILE-COMPONENT` Error** and `compile()` aborts (Tasks 6–7), matching the never-silent Global Constraint. (A component body is lines+staging; authoring one inside a staging-only `<timeline>` is out of v1 — the diagnostic makes that explicit rather than quietly narrowing scope.)
10. **`state[]` enum domains:** author enums emit their declared members; only implicit `scene.choices.*` entries append `"unset"` (§11.1) and carry `provenance: "branch:<id>"` (Task 12).
11. **`$` substitution:** a bare-path subject substitutes verbatim (matches §4.5's `scene.choices.number == 'blunt'`); a non-bare (compound) subject is parenthesized for precedence safety (Task 5).

## File Structure

```
crates/lute-compile/
  Cargo.toml            (Task 1)  deps: lute-{core-span,syntax,cel,manifest,check}, serde, serde_json, serde_yaml; dev: insta
  src/lib.rs            (T1,T12)  LUTE_IR_VERSION, module decls, compile() orchestration, envelope builders
  src/ir.rs             (Task 2)  Artifact, ArtifactMeta, StateEntry, Command (+14 kind structs), Stamp, Source, Role
  src/expand.rs         (Task 5)  D4: DefTable, expand_cel, expand_document (in-module unit tests)
  src/normalize.rs      (Task 6)  D8: ::use expansion + persist Set synthesis + component sentinels
  src/lower.rs          (Task 7)  Pass-1 per-primitive lowering, effective_wait, attr coercion
  src/cfg.rs            (Task 8)  Label, Rec, Emitter (symbolic-label machinery)
  src/stage.rs          (T8,9,10) walk_seq flatten → +injection/fork/join (T9) → +inline timeline (T10)
  src/schedule.rs       (Task 10) timeline clip scheduling over lute-check::resolve_timeline math
  src/address.rs        (Task 11) addr assignment, label resolution, lineId/voiceKey/code back-fill
  tests/ir_golden.rs    (Task 2)  per-kind serialization goldens
  tests/flatten.rs      (Task 8)  branch/match flatten shape
  tests/inject.rs       (Task 9)  4 injection rules + fork/join golden
  tests/timeline.rs     (Task 10) schedule order, stamps, barrier, stage-changing clip
  tests/address.rs      (Task 11) addr/label/lineId/voiceKey
  tests/compile.rs      (Task 12) gate, envelope, determinism
  tests/e2e.rs          (Task 14) bianca/showcase/components full-artifact insta goldens
crates/lute-check/
  src/match_check.rs    (Task 3)  E-CHOICE-DUP in check_branch (+unit test)
  src/check.rs          (Task 4)  FoldedEnv + fold_env() extraction (additive; check() refactored to call it)
  src/lib.rs            (Task 4)  re-export fold_env, FoldedEnv
  tests/fold_env.rs     (Task 4)  accessor integration test
crates/lute-cli/
  Cargo.toml            (Task 13) + lute-compile dep
  src/main.rs           (Task 13) Compile subcommand, build_input extraction, run_compile
  tests/compile.rs      (Task 13) exit codes 0/1/2, -o, stdout JSON
```

---

### Task 1: Scaffold the `lute-compile` crate (P0)

**Files:**
- Create: `crates/lute-compile/Cargo.toml`
- Create: `crates/lute-compile/src/lib.rs`

**Interfaces:**
- Consumes: workspace `Cargo.toml` member glob `members = ["crates/*"]` (already matches the new crate — no workspace edit needed); workspace deps `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `serde_yaml = "0.9"`, `insta = { version = "1", features = ["yaml"] }`.
- Produces: crate `lute-compile` with `pub const LUTE_IR_VERSION: &str = "0.0.1";` — Task 12's envelope stamps it.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/src/lib.rs`:

```rust
//! `lute-compile` — lowers a checked `.lute` document to the typed JSON
//! command-record artifact (design spec
//! `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`).

/// IR version stamped into every artifact envelope (`"lute": …`, spec §4.1).
pub const LUTE_IR_VERSION: &str = "0.0.1";

#[cfg(test)]
mod tests {
    #[test]
    fn ir_version_matches_language_version() {
        assert_eq!(super::LUTE_IR_VERSION, "0.0.1");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p lute-compile`
Expected: FAIL — `error: package(s) lute-compile not found in workspace` (no Cargo.toml yet).

- [ ] **Step 3: Write minimal implementation**

Create `crates/lute-compile/Cargo.toml`:

```toml
[package]
name = "lute-compile"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
lute-core-span = { path = "../lute-core-span" }
lute-syntax = { path = "../lute-syntax" }
lute-cel = { path = "../lute-cel" }
lute-manifest = { path = "../lute-manifest" }
lute-check = { path = "../lute-check" }
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }

[dev-dependencies]
insta = { workspace = true }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile`
Expected: PASS — `test tests::ir_version_matches_language_version ... ok` (1 passed).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings
git add crates/lute-compile/Cargo.toml crates/lute-compile/src/lib.rs
git commit -m "feat(compile): scaffold lute-compile crate"
```

---

### Task 2: Typed JSON IR (`ir.rs`, P1)

**Files:**
- Create: `crates/lute-compile/src/ir.rs`
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod ir;` + `pub use ir::*;`)
- Test: `crates/lute-compile/tests/ir_golden.rs`

**Interfaces:**
- Consumes: `lute_check::Provenance { pub injected: bool, pub by: String, pub reason: String }` (derives `Serialize`; re-exported at `lute_check` root).
- Produces (every later task references these EXACT names): `Artifact { lute: String, meta: ArtifactMeta, state: Vec<StateEntry>, commands: Vec<Command> }`; `ArtifactMeta { character: String, season: i64, episode: i64, episode_id: String, title: Option<String> }`; `StateEntry { path: String, ty: String, domain: Option<Vec<String>>, default: Option<serde_json::Value>, provenance: Option<String> }`; `enum Command { Line(LineCmd), Background(BackgroundCmd), Music(MusicCmd), Sfx(SfxCmd), Vfx(VfxCmd), Sprite(SpriteCmd), Camera(CameraCmd), Cut(CutCmd), Video(VideoCmd), Set(SetCmd), Choice(ChoiceCmd), Match(MatchCmd), Jump(JumpCmd), Barrier(BarrierCmd), Other(OtherCmd) }` with `kind` tag; `Stamp`, `Source`, `Role` (+ `Role::voiced()`); `Command::{addr_mut, for_each_target, stamp_mut}`.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/ir_golden.rs`:

```rust
//! Golden-per-kind serialization (spec §4.4): one exact-JSON assertion per
//! record kind pins the discriminator, camelCase field names, field order,
//! and None-field omission — the byte-stability contract everything else
//! (addresses, e2e goldens, determinism) rides on.

use std::collections::BTreeMap;

use lute_compile::*;

fn j(cmd: &Command) -> String {
    serde_json::to_string(cmd).unwrap()
}

#[test]
fn line_serializes_per_spec() {
    let cmd = Command::Line(LineCmd {
        addr: "002-0500".into(),
        role: Role::Dialogue,
        speaker: "bianca".into(),
        text: "Oh!".into(),
        emotion: Some("surprised".into()),
        variant: Some(0),
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "bianca.s01ep02.bianca_0010".into(),
        voice_key: Some("bianca-0010".into()),
        code: Some("0010".into()),
        stamp: Stamp::default(),
    });
    // `code` is #[serde(skip)] — the 3-id model (§4.2) admits no code field.
    assert_eq!(
        j(&cmd),
        r#"{"kind":"line","addr":"002-0500","role":"dialogue","speaker":"bianca","text":"Oh!","emotion":"surprised","variant":0,"lineId":"bianca.s01ep02.bianca_0010","voiceKey":"bianca-0010"}"#
    );
}

#[test]
fn unvoiced_line_has_no_voice_key() {
    let cmd = Command::Line(LineCmd {
        addr: "002-0400".into(),
        role: Role::Narration,
        speaker: "narrator".into(),
        text: "A hostess walked over.".into(),
        emotion: None,
        variant: None,
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "bianca.s01ep02.narrator_0010".into(),
        voice_key: None,
        code: None,
        stamp: Stamp::default(),
    });
    assert!(!j(&cmd).contains("voiceKey"));
    assert!(!Role::Narration.voiced());
    assert!(!Role::Monologue.voiced());
    assert!(Role::Dialogue.voiced());
    assert!(Role::Voiceover.voiced());
}

#[test]
fn injected_sprite_carries_provenance() {
    let cmd = Command::Sprite(SpriteCmd {
        addr: "002-0200".into(),
        character: "bianca".into(),
        anchor: None,
        action: None,
        exit: None,
        pos_reset: None,
        preload: Some(true),
        emotion: Some("surprised".into()),
        stamp: Stamp {
            provenance: Some(lute_check::Provenance {
                injected: true,
                by: "entry-emotion-lookahead".into(),
                reason: "pre-loading bianca's first emotion".into(),
            }),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"sprite","addr":"002-0200","character":"bianca","preload":true,"emotion":"surprised","provenance":{"injected":true,"by":"entry-emotion-lookahead","reason":"pre-loading bianca's first emotion"}}"#
    );
}

#[test]
fn choice_matches_spec_worked_example() {
    let cmd = Command::Choice(ChoiceCmd {
        addr: "004-0500".into(),
        branch_id: "number".into(),
        record_key: "scene.choices.number".into(),
        options: vec![ChoiceOption {
            id: "blunt".into(),
            label: "Just ask, flatly".into(),
            line_id: "bianca.s01ep02.number.blunt".into(),
            when: None,
            target: "004-0600".into(),
        }],
        converge: "004-1100".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"choice","addr":"004-0500","branchId":"number","recordKey":"scene.choices.number","options":[{"id":"blunt","label":"Just ask, flatly","lineId":"bianca.s01ep02.number.blunt","target":"004-0600"}],"converge":"004-1100"}"#
    );
}

#[test]
fn match_jump_barrier_serialize() {
    let m = Command::Match(MatchCmd {
        addr: "005-0700".into(),
        subject: "scene.choices.number".into(),
        arms: vec![MatchArm { test: "(scene.affect.bianca >= 1)".into(), target: "005-0800".into() }],
        otherwise: Some("005-1200".into()),
        converge: "005-1400".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&m),
        r#"{"kind":"match","addr":"005-0700","subject":"scene.choices.number","arms":[{"test":"(scene.affect.bianca >= 1)","target":"005-0800"}],"otherwise":"005-1200","converge":"005-1400"}"#
    );
    let jm = Command::Jump(JumpCmd { addr: "004-0700".into(), target: "004-1100".into() });
    assert_eq!(j(&jm), r#"{"kind":"jump","addr":"004-0700","target":"004-1100"}"#);
    let b = Command::Barrier(BarrierCmd { addr: "003-0800".into(), timeline: 1, at: 1.4 });
    assert_eq!(j(&b), r#"{"kind":"barrier","addr":"003-0800","timeline":1,"at":1.4}"#);
}

#[test]
fn stamped_camera_and_set_and_plugin_passthrough() {
    let cam = Command::Camera(CameraCmd {
        addr: "002-0300".into(),
        focus: Some("bianca".into()),
        zoom: Some(1.1),
        move_x: None,
        move_y: None,
        shake: None,
        reset: None,
        easing: None,
        stamp: Stamp { wait: Some(false), duration: Some(0.5), ..Stamp::default() },
    });
    assert_eq!(
        j(&cam),
        r#"{"kind":"camera","addr":"002-0300","focus":"bianca","zoom":1.1,"wait":false,"duration":0.5}"#
    );
    let set = Command::Set(SetCmd {
        addr: "004-0900".into(),
        path: "scene.affect.bianca".into(),
        op: "+=".into(),
        value: "1".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&set),
        r#"{"kind":"set","addr":"004-0900","path":"scene.affect.bianca","op":"+=","value":"1"}"#
    );
    let mut fields = BTreeMap::new();
    fields.insert("kind".to_string(), serde_json::Value::String("rhythm".into()));
    let other = Command::Other(OtherCmd { addr: "001-0100".into(), tag: "minigame".into(), fields, stamp: Stamp::default() });
    assert_eq!(
        j(&other),
        r#"{"kind":"plugin","addr":"001-0100","tag":"minigame","fields":{"kind":"rhythm"}}"#
    );
}

#[test]
fn timeline_stamp_and_source_flatten() {
    let cmd = Command::Vfx(VfxCmd {
        addr: "003-0500".into(),
        vfx_type: "whiteOut".into(),
        label: None,
        transition: Some("flash".into()),
        stamp: Stamp { at: Some(0.5), timeline: Some(1), source: Some(Source { component: "stinger".into() }), ..Stamp::default() },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"vfx","addr":"003-0500","vfxType":"whiteOut","transition":"flash","at":0.5,"timeline":1,"source":{"component":"stinger"}}"#
    );
}

#[test]
fn retarget_and_addr_helpers_visit_every_flow_field() {
    let mut cmd = Command::Choice(ChoiceCmd {
        addr: String::new(),
        branch_id: "b".into(),
        record_key: "scene.choices.b".into(),
        options: vec![ChoiceOption { id: "x".into(), label: "X".into(), line_id: String::new(), when: None, target: "@1".into() }],
        converge: "@2".into(),
        stamp: Stamp::default(),
    });
    *cmd.addr_mut() = "001-0100".into();
    let mut seen = Vec::new();
    cmd.for_each_target(&mut |t: &mut String| {
        seen.push(t.clone());
        *t = "RESOLVED".into();
    });
    assert_eq!(seen, vec!["@1".to_string(), "@2".to_string()]);
    assert!(!j(&cmd).contains('@'));
    assert!(cmd.stamp_mut().is_some());
    let mut jm = Command::Jump(JumpCmd { addr: String::new(), target: "@3".into() });
    assert!(jm.stamp_mut().is_none());
    let mut n = 0;
    jm.for_each_target(&mut |_| n += 1);
    assert_eq!(n, 1);
}

#[test]
fn envelope_serializes_with_state_entries() {
    let a = Artifact {
        lute: "0.0.1".into(),
        meta: ArtifactMeta { character: "bianca".into(), season: 1, episode: 2, episode_id: "S01EP02".into(), title: Some("T".into()) },
        state: vec![StateEntry {
            path: "scene.choices.number".into(),
            ty: "enum".into(),
            domain: Some(vec!["blunt".into(), "soft".into(), "unset".into()]),
            default: None,
            provenance: Some("branch:number".into()),
        }],
        commands: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        r#"{"lute":"0.0.1","meta":{"character":"bianca","season":1,"episode":2,"episodeId":"S01EP02","title":"T"},"state":[{"path":"scene.choices.number","type":"enum","domain":["blunt","soft","unset"],"provenance":"branch:number"}],"commands":[]}"#
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test ir_golden`
Expected: FAIL — `error[E0432]: unresolved import` / `cannot find type LineCmd` (ir module does not exist).

- [ ] **Step 3: Write minimal implementation**

Create `crates/lute-compile/src/ir.rs`:

```rust
//! Typed JSON IR (spec §4): tagged records with camelCase fields; only
//! relevant fields present (D3). Field DECLARATION ORDER is the serialized
//! order — part of the byte-stability contract; never reorder.

use std::collections::BTreeMap;

use serde::Serialize;

/// Envelope (§4.1): version + meta + folded state schema + flat command array.
#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub lute: String,
    pub meta: ArtifactMeta,
    pub state: Vec<StateEntry>,
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMeta {
    pub character: String,
    pub season: i64,
    pub episode: i64,
    pub episode_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// One folded state slot (§4.1): the engine's init/type table.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Cross-cutting optional stamps (§4.3), flattened into every stamped record:
/// resolved blocking, timing, timeline clip placement, injection provenance,
/// component source.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<lute_check::Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// `source { component }` on component-expanded records (§4.3, D8).
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub component: String,
}

/// `:line` role (§4.4). Voiced roles carry a `voiceKey` (§4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Dialogue,
    Narration,
    Monologue,
    Voiceover,
}

impl Role {
    pub fn voiced(self) -> bool {
        matches!(self, Role::Dialogue | Role::Voiceover)
    }
}

/// One record (§4.4). Internally tagged on `kind`; the `Other` variant is the
/// plugin-directive passthrough (plan spec-gap note 1) and serializes as
/// `kind: "plugin"`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Command {
    Line(LineCmd),
    Background(BackgroundCmd),
    Music(MusicCmd),
    Sfx(SfxCmd),
    Vfx(VfxCmd),
    Sprite(SpriteCmd),
    Camera(CameraCmd),
    Cut(CutCmd),
    Video(VideoCmd),
    Set(SetCmd),
    Choice(ChoiceCmd),
    Match(MatchCmd),
    Jump(JumpCmd),
    Barrier(BarrierCmd),
    #[serde(rename = "plugin")]
    Other(OtherCmd),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCmd {
    pub addr: String,
    pub role: Role,
    pub speaker: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog_motion: Option<String>,
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub as_label: Option<String>,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_key: Option<String>,
    /// Authored (or back-filled) per-speaker `code` — feeds `lineId`/`voiceKey`
    /// in the addressing pass, NEVER serialized (3-id model, §4.2).
    #[serde(skip)]
    pub code: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCmd {
    pub addr: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SfxCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VfxCmd {
    pub addr: String,
    pub vfx_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// Authored `::auto` OR an injected sprite command (§7.4) — injected records
/// are SEPARATE records with `provenance` in their stamp.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteCmd {
    pub addr: String,
    pub character: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shake: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCmd {
    pub addr: String,
    pub path: String,
    pub op: String,
    pub value: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceCmd {
    pub addr: String,
    pub branch_id: String,
    pub record_key: String,
    pub options: Vec<ChoiceOption>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCmd {
    pub addr: String,
    pub subject: String,
    pub arms: Vec<MatchArm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<String>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct MatchArm {
    pub test: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct JumpCmd {
    pub addr: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BarrierCmd {
    pub addr: String,
    pub timeline: u32,
    pub at: f64,
}

/// Plugin-directive passthrough (plan spec-gap note 1): `kind: "plugin"`,
/// the authored tag, and its attrs typed via the manifest `AttrDecl`s.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherCmd {
    pub addr: String,
    pub tag: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

impl Command {
    /// The record's `addr` slot (filled by the addressing pass, Task 11).
    pub fn addr_mut(&mut self) -> &mut String {
        match self {
            Command::Line(c) => &mut c.addr,
            Command::Background(c) => &mut c.addr,
            Command::Music(c) => &mut c.addr,
            Command::Sfx(c) => &mut c.addr,
            Command::Vfx(c) => &mut c.addr,
            Command::Sprite(c) => &mut c.addr,
            Command::Camera(c) => &mut c.addr,
            Command::Cut(c) => &mut c.addr,
            Command::Video(c) => &mut c.addr,
            Command::Set(c) => &mut c.addr,
            Command::Choice(c) => &mut c.addr,
            Command::Match(c) => &mut c.addr,
            Command::Jump(c) => &mut c.addr,
            Command::Barrier(c) => &mut c.addr,
            Command::Other(c) => &mut c.addr,
        }
    }

    /// Visit every control-flow target field (option/arm `target`s,
    /// `otherwise`, `converge`, jump `target`) — the addressing pass rewrites
    /// symbolic labels to concrete `addr`s through this single seam.
    pub fn for_each_target(&mut self, f: &mut impl FnMut(&mut String)) {
        match self {
            Command::Jump(j) => f(&mut j.target),
            Command::Choice(c) => {
                for o in &mut c.options {
                    f(&mut o.target);
                }
                f(&mut c.converge);
            }
            Command::Match(m) => {
                for a in &mut m.arms {
                    f(&mut a.target);
                }
                if let Some(o) = &mut m.otherwise {
                    f(o);
                }
                f(&mut m.converge);
            }
            _ => {}
        }
    }

    /// The record's stamp, when it has one (`jump`/`barrier` do not).
    pub fn stamp_mut(&mut self) -> Option<&mut Stamp> {
        match self {
            Command::Line(c) => Some(&mut c.stamp),
            Command::Background(c) => Some(&mut c.stamp),
            Command::Music(c) => Some(&mut c.stamp),
            Command::Sfx(c) => Some(&mut c.stamp),
            Command::Vfx(c) => Some(&mut c.stamp),
            Command::Sprite(c) => Some(&mut c.stamp),
            Command::Camera(c) => Some(&mut c.stamp),
            Command::Cut(c) => Some(&mut c.stamp),
            Command::Video(c) => Some(&mut c.stamp),
            Command::Set(c) => Some(&mut c.stamp),
            Command::Choice(c) => Some(&mut c.stamp),
            Command::Match(c) => Some(&mut c.stamp),
            Command::Other(c) => Some(&mut c.stamp),
            Command::Jump(_) | Command::Barrier(_) => None,
        }
    }
}
```

Modify `crates/lute-compile/src/lib.rs` — add directly under the module doc comment:

```rust
pub mod ir;

pub use ir::*;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile --test ir_golden`
Expected: PASS — 8 tests ok. If any exact-string assertion fails on FIELD ORDER, fix the struct declaration order (never the test): declaration order IS the contract.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/ir.rs crates/lute-compile/src/lib.rs crates/lute-compile/tests/ir_golden.rs
git commit -m "feat(compile): typed JSON IR record types + serialization goldens"
```

---

### Task 3: `E-CHOICE-DUP` in `lute-check` (P2a)

The D6 compile gate relies on the checker proving `<choice id>` uniqueness within a branch (dsl §11.1: "Within a `<branch>`, each `<choice id>` MUST be unique — duplicate choice ids are a static error"). This diagnostic does not exist yet.

**Files:**
- Modify: `crates/lute-check/src/match_check.rs:185-211` (`check_branch`) + its `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes (existing, unchanged): `pub fn check_branch(branch: &Branch, seen: &mut BTreeSet<String>) -> BranchRecord` with `BranchRecord { pub path: String, pub decl: StateDecl, pub diags: Vec<Diagnostic> }`; diags already propagate through `check.rs::fold_branches_nodes` into `check()`'s diagnostic stream — NO caller changes.
- Produces: `E-CHOICE-DUP` (`Severity::Error`, `Layer::Logic`, one per duplicate `<choice>`, at the duplicate choice's span) inside `BranchRecord.diags`.

- [ ] **Step 1: Write the failing test**

Append to the existing `#[cfg(test)] mod tests` at the bottom of `crates/lute-check/src/match_check.rs` (it already has `use super::*;`; add the `Choice` import inside the test fn):

```rust
    #[test]
    fn duplicate_choice_ids_flag_e_choice_dup() {
        use lute_syntax::ast::Choice;
        let sp = Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        };
        let choice = |id: &str| Choice {
            id: id.into(),
            label: id.into(),
            when: None,
            attrs: Vec::new(),
            body: Vec::new(),
            span: sp,
        };
        let branch = Branch {
            id: "number".into(),
            attrs: Vec::new(),
            choices: vec![choice("blunt"), choice("soft"), choice("blunt")],
            span: sp,
        };
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch, &mut seen);
        let dups: Vec<_> = rec.diags.iter().filter(|d| d.code == "E-CHOICE-DUP").collect();
        assert_eq!(dups.len(), 1, "exactly one E-CHOICE-DUP for the one repeat id");
        assert_eq!(dups[0].severity, Severity::Error);
        assert!(dups[0].message.contains("blunt"), "{}", dups[0].message);

        // Unique ids stay clean.
        let ok = Branch {
            id: "other".into(),
            attrs: Vec::new(),
            choices: vec![choice("a"), choice("b")],
            span: sp,
        };
        let rec = check_branch(&ok, &mut seen);
        assert!(rec.diags.iter().all(|d| d.code != "E-CHOICE-DUP"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-check duplicate_choice_ids_flag_e_choice_dup`
Expected: FAIL — `assertion ... exactly one E-CHOICE-DUP for the one repeat id` (0 found).

- [ ] **Step 3: Write minimal implementation**

In `check_branch` (match_check.rs:185), insert the duplicate-choice scan between the existing `E-DUP-BRANCH` block and the `// Implicit decl:` comment:

```rust
    // E-CHOICE-DUP (dsl §11.1): each `<choice id>` MUST be unique within its
    // branch — both the recorded value's domain and the option-label lineId
    // (`{branchId}.{choiceId}`, §12) key on it. One diagnostic per repeat, at
    // the duplicate choice's span.
    let mut choice_ids: BTreeSet<&str> = BTreeSet::new();
    for choice in &branch.choices {
        if !choice_ids.insert(choice.id.as_str()) {
            diags.push(diag(
                "E-CHOICE-DUP",
                Severity::Error,
                format!(
                    "duplicate `<choice id=\"{}\">` within `<branch id=\"{}\">`; choice ids \
                     must be unique within a branch (dsl §11.1)",
                    choice.id, branch.id
                ),
                choice.span,
            ));
        }
    }
```

(The module-local `diag(code, severity, message, span)` helper already exists and builds a `Layer::Logic` diagnostic.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-check duplicate_choice_ids_flag_e_choice_dup`
Expected: PASS.
Then run the full crate + dependents (additive-change guard): `cargo test -p lute-check -p lute-lsp -p lute-cli`
Expected: PASS — all existing suites green (no fixture contains a duplicate choice id).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/match_check.rs
git commit -m "feat(check): E-CHOICE-DUP - unique <choice id> within a branch (dsl 11.1)"
```

---

### Task 4: `fold_env` accessor in `lute-check` (P2b)

Spec §11 ("Reuse-input exposure"): the folded state schema + merged def tables live inside `check()`'s body; `lute-compile` needs read access. Extract the fold into a public `fold_env()` that `check()` itself calls — one source of truth, additive surface.

**Files:**
- Modify: `crates/lute-check/src/check.rs` (extract lines ~192–326: from `let (typed, meta_diags) = parse_meta(&doc.meta, &input.snapshot);` through the `let env = Env { … };` construction)
- Modify: `crates/lute-check/src/lib.rs:16` (extend the `pub use check::…` re-export)
- Test: `crates/lute-check/tests/fold_env.rs`

**Interfaces:**
- Consumes (existing): `parse_meta(&Meta, &CapabilitySnapshot) -> (TypedMeta, Vec<Diagnostic>)`; `TypedMeta { pub state: StateSchema, pub defs: BTreeMap<String, serde_yaml::Value>, pub character: Option<String>, pub season: Option<i64>, pub episode: Option<i64>, … }`; `SchemaImports { pub state: StateSchema, pub defs: BTreeMap<String, serde_yaml::Value>, pub state_overridable: BTreeSet<String>, pub diags: Vec<Diagnostic> }`; `CapabilitySnapshot { pub defs: BTreeMap<String, DefDecl>, … }` with `DefDecl { pub name: String, pub ty: Type, pub params: Vec<DefParam>, pub cel: String, … }`; `Env { pub mode: Mode, pub state: StateSchema, pub defs: BTreeSet<String>, pub def_types: BTreeMap<String, Type>, pub def_params: BTreeMap<String, Vec<(String, Type)>> }`; the private `fold_branches` / `fold_directive_slots` / `params_from_yaml` helpers (stay private; `fold_env` lives in the same module).
- Produces (Task 12 consumes): 

```rust
pub struct FoldedEnv {
    pub typed: TypedMeta,
    pub env: Env,
    /// def name -> raw CEL body, merged plugin < imported < inline (D4 input).
    pub def_bodies: BTreeMap<String, String>,
}
pub fn fold_env(doc: &Document, input: &CheckInput) -> (FoldedEnv, Vec<Diagnostic>)
```

- [ ] **Step 1: Write the failing test**

Create `crates/lute-check/tests/fold_env.rs`:

```rust
//! `fold_env` accessor (compile-spec §11 reuse-input exposure): the FOLDED
//! state schema (inline + implicit `scene.choices.*`) and the merged def
//! tables (types, params, CEL bodies) surface through one public call.

use lute_check::{fold_env, CheckInput, Mode};
use lute_core_span::Severity;
use lute_manifest::types::Type;

const SCENE: &str = r#"---
character: bianca
season: 1
episode: 2
state:
  scene.affect.bianca: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.bianca >= 1" }
---

## Shot 1.

<branch id="number">
  <choice id="blunt" label="Flat">
    :line[fixer]: a
  </choice>
  <choice id="soft" label="Gentle">
    :line[fixer]: b
  </choice>
</branch>
"#;

#[test]
fn fold_env_exposes_folded_schema_and_def_bodies() {
    let input = CheckInput {
        text: SCENE.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    };
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, diags) = fold_env(&doc, &input);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    // Inline decl folded.
    assert!(folded.env.state.decls.contains_key("scene.affect.bianca"));
    // Implicit branch decl folded (§11.1).
    let choice = folded
        .env
        .state
        .decls
        .get("scene.choices.number")
        .expect("implicit branch decl folded");
    assert_eq!(
        choice.ty,
        Type::Enum(vec!["blunt".to_string(), "soft".to_string()])
    );
    // Def body exposed for the D4 expander.
    assert_eq!(
        folded.def_bodies.get("fond").map(String::as_str),
        Some("scene.affect.bianca >= 1")
    );
    assert_eq!(folded.env.def_types.get("fond"), Some(&Type::Bool));
    // Typed frontmatter rides along for the envelope.
    assert_eq!(folded.typed.character.as_deref(), Some("bianca"));
    assert_eq!(folded.typed.season, Some(1));
    assert_eq!(folded.typed.episode, Some(2));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-check --test fold_env`
Expected: FAIL — `error[E0432]: unresolved import lute_check::fold_env`.

- [ ] **Step 3: Write minimal implementation**

In `crates/lute-check/src/check.rs`, directly ABOVE `pub fn check(…)`, add the struct + function. The function body is the EXISTING check() code MOVED VERBATIM — cut everything from the line `let (typed, meta_diags) = parse_meta(&doc.meta, &input.snapshot);` (under the `// 3. Typed frontmatter…` comment) through the `let env = Env { mode: input.mode, state: schema, defs, def_types, def_params };` statement inclusive — plus the NEW `def_bodies` fold and the return:

```rust
/// The folded compile inputs `check()` builds internally (compile-spec §11
/// reuse-input exposure): the typed frontmatter, the analysis [`Env`] whose
/// `state` is the FOLDED schema (imported ∪ inline ∪ implicit
/// `scene.choices.*` ∪ plugin-declared slots), and the merged def CEL bodies
/// (`plugin < imported < inline`, mirroring `def_types`). One source of
/// truth: `check()` itself consumes this fold.
#[derive(Clone, Debug)]
pub struct FoldedEnv {
    pub typed: crate::meta::TypedMeta,
    pub env: Env,
    pub def_bodies: std::collections::BTreeMap<String, String>,
}

/// Fold the analysis environment from an already-parsed document. Returns the
/// fold's diagnostics (meta + state-merge + branch dup/choice-dup) so `check()`
/// and external callers report identically. Pure and total; never panics.
pub fn fold_env(doc: &Document, input: &CheckInput) -> (FoldedEnv, Vec<Diagnostic>) {
    // >>> the MOVED block from check() goes here, verbatim, with two edits:
    //     1. its `meta_diags` / `state_merge_diags` / `branch_diags` locals are
    //        appended into one `fold_diags` accumulator in that order;
    //     2. after the `def_params` fold, add the def-bodies fold below.
    // (moved code not repeated here — it is cut, not copied)

    // def name -> raw CEL body for the D4 expander. Same three sources and the
    // same precedence as `def_types`: plugin < imported < inline.
    let mut def_bodies: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for (name, d) in &input.snapshot.defs {
        def_bodies.insert(name.clone(), d.cel.clone());
    }
    for (name, v) in &input.imports.defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }
    for (name, v) in &typed.defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }

    let env = Env {
        mode: input.mode,
        state: schema,
        defs,
        def_types,
        def_params,
    };
    (
        FoldedEnv {
            typed,
            env,
            def_bodies,
        },
        fold_diags,
    )
}
```

Concretely, inside the moved block: replace `let (typed, meta_diags) = …;` with `let (typed, mut fold_diags) = parse_meta(&doc.meta, &input.snapshot);`, replace every `state_merge_diags.push(…)` with `fold_diags.push(…)` (delete the `let mut state_merge_diags …` declaration), and change `fold_branches(&doc, …, &mut branch_diags)` to `fold_branches(doc, &mut schema, &mut seen_branches, &mut fold_diags)` (deleting `let mut branch_diags …` and `let mut seen_branches` stays). `fold_directive_slots(&doc, …)` becomes `fold_directive_slots(doc, &input.snapshot, &mut schema)`.

Then rewrite the corresponding section of `check()` to consume it — after the `// 3.` comment block, the whole moved region collapses to:

```rust
    // 3–4b. Typed frontmatter + folded schema + merged def tables (one SoT:
    // the public fold_env accessor the compiler also consumes).
    let (folded, fold_diags) = fold_env(&doc, input);
    let env = &folded.env;
    let base_ctx = Ctx {
        env,
        in_match: false,
        match_subject: None,
    };
```

Downstream in `check()`: `check_definite_assignment(&all_nodes, &env.state, &base_ctx)` already reads through `env`; in the diagnostic collection replace the three lines `diags.extend(meta_diags); … diags.extend(branch_diags); … diags.extend(state_merge_diags);` with a single `diags.extend(fold_diags);` placed where `meta_diags` was. (All diagnostics are byte+code sorted before return, so the grouping change cannot reorder the final stream.)

Finally, `crates/lute-check/src/lib.rs:16` — extend the re-export:

```rust
pub use check::{check, fold_env, CheckInput, CheckResult, FoldedEnv, Resolved};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-check --test fold_env`
Expected: PASS.
Then the refactor guard — the whole checker surface must be byte-identical: `cargo test -p lute-check -p lute-lsp -p lute-cli`
Expected: PASS, including every insta golden in `lute-check/tests/golden.rs` and `crates/lute-lsp/tests/divergence.rs`. Any golden diff here means the fold changed behavior — fix the extraction, never the snapshots.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/check.rs crates/lute-check/src/lib.rs crates/lute-check/tests/fold_env.rs
git commit -m "feat(check): fold_env - expose folded schema + merged def tables (compile reuse)"
```

---

### Task 5: `@ref`/`@fn(args)`/`$` inline-CEL expander (`expand.rs`, P3 / D4)

NOT `substitute_dsl_tokens` (that private `lute-cel` helper only blanks tokens for parser prep). This is a real macro expander over the merged def table: typed positional binding, recursive, cycle-guarded, parenthesizing every substituted body; then `$` → the enclosing `<match>` subject. Output CEL is `@`/`$`-free (D4).

**Files:**
- Create: `crates/lute-compile/src/expand.rs` (implementation + in-module unit tests)
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod expand;`)

**Interfaces:**
- Consumes: `lute_cel::scan_refs(raw: &str) -> Vec<RefUse>` with `RefUse { pub name: String, pub is_dollar: bool, pub span: Span, pub call: Option<Call> }`, `Call { pub span: Span, pub args: Vec<Span> }` (byte spans into `raw`; `@`/`$` inside CEL string literals are already skipped); `lute_cel::cel_string_mask(raw: &str) -> Vec<bool>`; AST types `Document, Node, Arm, Attr, AttrValue, CelSlot, ClipNode` from `lute_syntax::ast`; `Diagnostic`/`Severity`/`Layer` from `lute_core_span`; `Type` from `lute_manifest::types`.
- Produces (Task 12 consumes):

```rust
pub struct DefTable<'a> {
    pub bodies: &'a BTreeMap<String, String>,
    pub params: &'a BTreeMap<String, Vec<(String, Type)>>,
}
pub fn expand_document(doc: &mut Document, defs: &DefTable<'_>) -> Vec<Diagnostic>
pub fn expand_cel(raw: &str, defs: &DefTable<'_>, subject: Option<&str>, stack: &mut Vec<String>) -> Result<String, String>
```

- [ ] **Step 1: Write the failing tests**

Create `crates/lute-compile/src/expand.rs` with ONLY the test module first (the impl lands in Step 3 above it):

```rust
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lute_manifest::types::Type;

    use super::*;

    type Tables = (BTreeMap<String, String>, BTreeMap<String, Vec<(String, Type)>>);

    fn tables(bodies: &[(&str, &str)], params: &[(&str, &[&str])]) -> Tables {
        let b = bodies
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let p = params
            .iter()
            .map(|(k, ps)| {
                (
                    k.to_string(),
                    ps.iter().map(|n| (n.to_string(), Type::Number)).collect(),
                )
            })
            .collect();
        (b, p)
    }

    fn expand(raw: &str, t: &Tables, subject: Option<&str>) -> Result<String, String> {
        let defs = DefTable { bodies: &t.0, params: &t.1 };
        expand_cel(raw, &defs, subject, &mut Vec::new())
    }

    #[test]
    fn bare_ref_expands_parenthesized() {
        let t = tables(&[("fond", "scene.affect.bianca >= 1")], &[]);
        assert_eq!(expand("@fond", &t, None).unwrap(), "(scene.affect.bianca >= 1)");
    }

    #[test]
    fn fn_ref_binds_args_positionally_and_parenthesized() {
        let t = tables(&[("atLeast", "scene.affect.bianca >= n")], &[("atLeast", &["n"])]);
        assert_eq!(
            expand("@atLeast(2)", &t, None).unwrap(),
            "(scene.affect.bianca >= (2))"
        );
        // Param ident boundaries: `n` inside `scene.n`/`none` must NOT bind.
        let t = tables(&[("f", "scene.n + none + n")], &[("f", &["n"])]);
        assert_eq!(expand("@f(9)", &t, None).unwrap(), "(scene.n + none + (9))");
    }

    #[test]
    fn refs_expand_recursively() {
        let t = tables(&[("a", "@b + 1"), ("b", "2")], &[]);
        assert_eq!(expand("@a", &t, None).unwrap(), "((2) + 1)");
    }

    #[test]
    fn cycle_is_an_error_not_a_hang() {
        let t = tables(&[("a", "@b"), ("b", "@a")], &[]);
        let err = expand("@a", &t, None).unwrap_err();
        assert!(err.contains("cycle"), "{err}");
    }

    #[test]
    fn dollar_substitutes_bare_subject_verbatim() {
        let t = tables(&[], &[]);
        assert_eq!(
            expand("$ == 'blunt'", &t, Some("scene.choices.number")).unwrap(),
            "scene.choices.number == 'blunt'"
        );
        // Compound subject gets parenthesized (plan spec-gap note 11).
        assert_eq!(
            expand("$ == 3", &t, Some("a + b")).unwrap(),
            "(a + b) == 3"
        );
        // `$` with no enclosing match is a gate-proven-unreachable error.
        assert!(expand("$ == 1", &t, None).is_err());
    }

    #[test]
    fn string_literal_tokens_are_untouched() {
        let t = tables(&[], &[]);
        assert_eq!(
            expand("x == '@gold'", &t, None).unwrap(),
            "x == '@gold'"
        );
    }

    #[test]
    fn unknown_ref_is_an_error() {
        let t = tables(&[], &[]);
        assert!(expand("@nope", &t, None).is_err());
    }

    #[test]
    fn expand_document_rewrites_slots_with_match_subject_scope() {
        let src = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n\n## Shot 1.\n\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :line[fixer]{delivery=\"thought\"}: a\n  </when>\n  <when test=\"$ == 'blunt'\">\n    :line[fixer]{delivery=\"thought\"}: b\n  </when>\n  <otherwise>\n    :line[fixer]{delivery=\"thought\"}: c\n  </otherwise>\n</match>\n";
        let (mut doc, diags) = lute_syntax::parse(src);
        assert!(diags.iter().all(|d| d.severity != lute_core_span::Severity::Error));
        let t = tables(&[("fond", "scene.affect.bianca >= 1")], &[]);
        let defs = DefTable { bodies: &t.0, params: &t.1 };
        let ediags = expand_document(&mut doc, &defs);
        assert!(ediags.is_empty(), "{ediags:#?}");
        let lute_syntax::ast::Node::Match(m) = &doc.shots[0].body[0] else {
            panic!("first node is the match");
        };
        let tests: Vec<&str> = m
            .arms
            .iter()
            .filter_map(|a| match a {
                lute_syntax::ast::Arm::When { test, .. } => Some(test.raw.as_str()),
                lute_syntax::ast::Arm::Otherwise { .. } => None,
            })
            .collect();
        assert_eq!(
            tests,
            vec![
                "(scene.affect.bianca >= 1)",
                "scene.choices.number == 'blunt'"
            ]
        );
    }
}
```

Add `pub mod expand;` to `crates/lute-compile/src/lib.rs` (below `pub mod ir;`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-compile expand`
Expected: FAIL — `error[E0425]: cannot find function expand_cel` (compile error; module has tests only).

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/lute-compile/src/expand.rs` (above the test module):

```rust
//! D4: compile-time `@ref`/`@fn(args)`/`$` → inline-CEL expansion.
//!
//! A def ref is a compile-time macro (dsl §8.1). Each substituted body is
//! PARENTHESIZED; `@fn(args)` binds args positionally (arity/type already
//! gate-proven by the checker); expansion recurses with a cycle guard; `$`
//! substitutes the enclosing `<match>` subject. The artifact carries no defs
//! table — output CEL is `@`/`$`-free.

use std::collections::BTreeMap;

use lute_cel::{cel_string_mask, scan_refs};
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, Attr, AttrValue, CelSlot, ClipNode, Document, Node};

/// The merged def table (plugin < imported < inline), borrowed from
/// `lute_check::FoldedEnv { def_bodies, env.def_params }`.
pub struct DefTable<'a> {
    pub bodies: &'a BTreeMap<String, String>,
    pub params: &'a BTreeMap<String, Vec<(String, Type)>>,
}

/// Expand every CEL slot in the document in place. Returns diagnostics for
/// expander failures (`E-COMPILE-EXPAND`: cycle / unknown def / arity — the
/// latter two gate-proven unreachable, kept total). Never panics.
pub fn expand_document(doc: &mut Document, defs: &DefTable<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for shot in &mut doc.shots {
        expand_nodes(&mut shot.body, defs, None, &mut diags);
    }
    diags
}

fn expand_nodes(
    nodes: &mut [Node],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Line(l) => expand_attrs(&mut l.attrs, defs, subject, diags),
            Node::Directive(d) => expand_attrs(&mut d.attrs, defs, subject, diags),
            Node::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
            Node::Branch(b) => {
                expand_attrs(&mut b.attrs, defs, subject, diags);
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        expand_slot(w, defs, subject, diags);
                    }
                    expand_attrs(&mut c.attrs, defs, subject, diags);
                    expand_nodes(&mut c.body, defs, subject, diags);
                }
            }
            Node::Match(m) => {
                // The subject itself expands in the OUTER scope (a nested
                // match's `$` refers to its own subject only after this).
                expand_slot(&mut m.subject, defs, subject, diags);
                let inner = m.subject.raw.clone();
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            expand_slot(test, defs, Some(&inner), diags);
                            expand_nodes(body, defs, Some(&inner), diags);
                        }
                        Arm::Otherwise { body, .. } => {
                            expand_nodes(body, defs, Some(&inner), diags)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                if let Some(d) = &mut t.duration {
                    expand_slot(d, defs, subject, diags);
                }
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => expand_attrs(&mut d.attrs, defs, subject, diags),
                            ClipNode::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
                        }
                    }
                }
            }
        }
    }
}

fn expand_attrs(
    attrs: &mut [Attr],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in attrs {
        if let AttrValue::Ref(slot) = &mut a.value {
            expand_slot(slot, defs, subject, diags);
        }
    }
}

fn expand_slot(
    slot: &mut CelSlot,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    match expand_cel(&slot.raw, defs, subject, &mut Vec::new()) {
        Ok(s) => slot.raw = s,
        Err(message) => diags.push(Diagnostic {
            code: "E-COMPILE-EXPAND".to_string(),
            severity: Severity::Error,
            message,
            span: slot.span,
            layer: Layer::Cel,
            fixits: Vec::new(),
            provenance: None,
        }),
    }
}

/// Expand one raw CEL fragment. `stack` is the def-name expansion path (cycle
/// guard). On `Err` the stack may be left dirty — the caller aborts the whole
/// compile, never resumes.
pub fn expand_cel(
    raw: &str,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let refs = scan_refs(raw);
    if refs.is_empty() {
        return Ok(raw.to_string());
    }
    let mut out = raw.to_string();
    // Right-to-left so earlier byte offsets stay valid while splicing.
    for r in refs.iter().rev() {
        let end = r.call.as_ref().map_or(r.span.byte_end, |c| c.span.byte_end);
        let replacement = if r.is_dollar {
            let Some(s) = subject else {
                return Err("`$` used outside a <match> arm".to_string());
            };
            subject_text(s)
        } else {
            expand_ref(r, raw, defs, subject, stack)?
        };
        out.replace_range(r.span.byte_start..end, &replacement);
    }
    Ok(out)
}

fn expand_ref(
    r: &lute_cel::RefUse,
    raw: &str,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    let name = &r.name;
    let Some(body) = defs.bodies.get(name) else {
        return Err(format!("`@{name}` names no known def body (gate should have caught this)"));
    };
    // Args expand in the CALLER's scope, BEFORE the cycle push — `@f(@f(1))`
    // is nesting, not a cycle.
    let params = defs.params.get(name).cloned().unwrap_or_default();
    let args: Vec<String> = match &r.call {
        Some(call) => {
            let mut v = Vec::with_capacity(call.args.len());
            for sp in &call.args {
                v.push(expand_cel(&raw[sp.byte_start..sp.byte_end], defs, subject, stack)?);
            }
            v
        }
        None => Vec::new(),
    };
    if args.len() != params.len() {
        return Err(format!(
            "`@{name}` takes {} arg(s), got {} (gate should have caught this)",
            params.len(),
            args.len()
        ));
    }
    if stack.iter().any(|n| n == name) {
        return Err(format!(
            "def expansion cycle: {} -> {name}",
            stack.join(" -> ")
        ));
    }
    stack.push(name.clone());
    // A def body is subject-independent (no `$`); nested `@refs` recurse.
    let mut expanded = expand_cel(body, defs, None, stack)?;
    for ((pname, _ty), arg) in params.iter().zip(&args) {
        expanded = substitute_ident(&expanded, pname, &format!("({arg})"));
    }
    stack.pop();
    Ok(format!("({expanded})"))
}

/// Replace whole-identifier occurrences of `name` outside CEL string literals.
/// An occurrence preceded by `.`/ident-byte or followed by an ident-byte is a
/// different identifier (`scene.n`, `none`) and is left alone.
fn substitute_ident(body: &str, name: &str, replacement: &str) -> String {
    let mask = cel_string_mask(body);
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if !mask[i] && body[i..].starts_with(name) {
            let prev_ok = i == 0 || {
                let p = bytes[i - 1];
                !(p.is_ascii_alphanumeric() || p == b'_' || p == b'.')
            };
            let end = i + name.len();
            let next_ok = end >= bytes.len() || {
                let n = bytes[end];
                !(n.is_ascii_alphanumeric() || n == b'_')
            };
            if prev_ok && next_ok {
                out.push_str(replacement);
                i = end;
                continue;
            }
        }
        let ch_len = body[i..].chars().next().map_or(1, |c| c.len_utf8());
        out.push_str(&body[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// `$` substitution text (§4.5): a bare dotted path goes in verbatim; anything
/// compound is parenthesized for precedence safety.
fn subject_text(subject: &str) -> String {
    let bare = !subject.is_empty()
        && subject
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if bare {
        subject.to_string()
    } else {
        format!("({subject})")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-compile expand`
Expected: PASS — 8 tests ok.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/expand.rs crates/lute-compile/src/lib.rs
git commit -m "feat(compile): @ref/@fn/$ inline-CEL expander (D4)"
```

---

### Task 6: AST normalization — `::use` expansion + persist `Set` synthesis (`normalize.rs`, P4 / D8)

Components and choice-`persist` rewrite the REAL node tree BEFORE lowering (D8), so expanded nodes participate in `@ref` expansion, lowering, auto-injection, and lookahead. Component-sourced regions are delimited by sentinel directives (`__component-begin`/`__component-end`) the stage walker consumes into `source { component }` stamps — the AST has no provenance field (plan File Structure; sentinels never reach the artifact).

**Files:**
- Create: `crates/lute-compile/src/normalize.rs` (implementation + in-module unit tests)
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod normalize;`)

**Interfaces:**
- Consumes: `lute_check::{ComponentSet, ComponentDef}` — `ComponentSet { pub table: BTreeMap<String, ComponentDef>, pub diags: Vec<Diagnostic> }`, `ComponentDef { pub params: Vec<(String, Type)>, pub body: Document, pub src: PathBuf }`; `lute_check::meta::{StateSchema, StateDecl}` (`StateDecl { pub ty: Type, pub default: Option<Literal>, pub namespace: Namespace }`); `lute_check::resolve_components(base_dir: &Path, components: &[String], at: Span) -> ComponentSet` (test only); AST types incl. `CelSlot::raw(kind, raw, span)` and `CelKind::SetExpr`; `lute_cel::scan_refs`.
- Produces (Tasks 7/8/12 consume):

```rust
pub const COMPONENT_BEGIN: &str = "__component-begin";
pub const COMPONENT_END: &str = "__component-end";
pub fn normalize_document(doc: &mut Document, components: &ComponentSet, schema: &StateSchema) -> Vec<Diagnostic>
pub fn cel_string_literal(s: &str) -> String
```

- [ ] **Step 1: Write the failing tests**

Create `crates/lute-compile/src/normalize.rs` with ONLY the test module first:

```rust
#[cfg(test)]
mod tests {
    use std::path::Path;

    use lute_check::meta::{Namespace, StateDecl, StateSchema};
    use lute_check::resolve_components;
    use lute_core_span::Severity;
    use lute_manifest::types::{Literal, Type};
    use lute_syntax::ast::{AttrValue, Node};

    use super::*;

    fn parse_clean(src: &str) -> lute_syntax::ast::Document {
        let (doc, diags) = lute_syntax::parse(src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc
    }

    #[test]
    fn use_expands_component_inline_with_bound_params_and_sentinels() {
        // Real fixture: docs/examples/components/greet.component.lute declares
        // `component: greet`, `params: { who: string }`, body =
        // `::auto{character=@who action="fade-in-up"}` + a narrator line.
        let base = Path::new("../../docs/examples/components");
        let scene = std::fs::read_to_string(base.join("scene.lute")).unwrap();
        let mut doc = parse_clean(&scene);
        let comps = resolve_components(
            base,
            &["greet.component.lute".to_string()],
            doc.meta.span,
        );
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let body = &doc.shots[0].body;
        // ::use replaced by: begin sentinel, ::auto (param bound), line, end sentinel, then the scene's own line.
        let tags: Vec<String> = body
            .iter()
            .map(|n| match n {
                Node::Directive(d) => format!("::{}", d.tag),
                Node::Line(l) => format!(":line[{}]", l.speaker),
                _ => "other".to_string(),
            })
            .collect();
        assert_eq!(
            tags,
            vec![
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                ":line[narrator]".to_string(),
                format!("::{COMPONENT_END}"),
                ":line[narrator]".to_string(),
            ]
        );
        // `character=@who` became the VALUE-LEVEL string arg (whole-slot bind).
        let Node::Directive(auto) = &body[1] else { panic!("auto") };
        let ch = auto.attrs.iter().find(|a| a.key == "character").unwrap();
        assert!(matches!(&ch.value, AttrValue::Str(s) if s == "bianca"), "{ch:?}");
        // No `::use` survives normalization (D8).
        assert!(body.iter().all(|n| !matches!(n, Node::Directive(d) if d.tag == "use")));
    }

    #[test]
    fn persist_synthesizes_trailing_set_nodes() {
        let src = r#"---
character: sofia
season: 1
episode: 1
---

## Shot 1.

<branch id="sofaHelp">
  <choice id="help" label="Help her up" persist="run" as="run.metHelpfully">
    :line[sofia]: Thank you.
  </choice>
  <choice id="warmly" label="Stay a while" persist="run" as="run.outcome" value="warm">
    :line[sofia]: Kind.
  </choice>
  <choice id="tip" label="Leave a tip" persist="run" as="run.tip" value="5">
    :line[sofia]: Oh.
  </choice>
</branch>
"#;
        let mut doc = parse_clean(src);
        let mut schema = StateSchema::default();
        schema.decls.insert(
            "run.metHelpfully".to_string(),
            StateDecl { ty: Type::Bool, default: Some(Literal::Bool(false)), namespace: Namespace::Run },
        );
        schema.decls.insert(
            "run.outcome".to_string(),
            StateDecl { ty: Type::Enum(vec!["warm".into(), "cold".into()]), default: None, namespace: Namespace::Run },
        );
        schema.decls.insert(
            "run.tip".to_string(),
            StateDecl { ty: Type::Number, default: None, namespace: Namespace::Run },
        );
        let diags = normalize_document(&mut doc, &Default::default(), &schema);
        assert!(diags.is_empty(), "{diags:#?}");

        let Node::Branch(b) = &doc.shots[0].body[0] else { panic!("branch") };
        let last_set = |i: usize| -> (&str, &str, &str) {
            let Some(Node::Set(s)) = b.choices[i].body.last() else {
                panic!("choice {i} ends in a synthesized Set");
            };
            (s.path.as_str(), s.op.as_str(), s.expr.raw.as_str())
        };
        // bool target, no value => `= true` (dsl §11.1.1 rule 4).
        assert_eq!(last_set(0), ("run.metHelpfully", "=", "true"));
        // enum target => quoted CEL string literal.
        assert_eq!(last_set(1), ("run.outcome", "=", "'warm'"));
        // number target => bare numeric literal.
        assert_eq!(last_set(2), ("run.tip", "=", "5"));
        // The authored line plus exactly one synthesized Set per persisting choice.
        assert_eq!(b.choices[0].body.len(), 2);
    }

    #[test]
    fn cel_string_literal_escapes_quotes_and_backslashes() {
        assert_eq!(cel_string_literal("warm"), "'warm'");
        assert_eq!(cel_string_literal("it's"), "'it\\'s'");
        assert_eq!(cel_string_literal("a\\b"), "'a\\\\b'");
    }
}
```

Add `pub mod normalize;` to `crates/lute-compile/src/lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-compile normalize`
Expected: FAIL — `error[E0425]: cannot find function normalize_document`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/lute-compile/src/normalize.rs`:

```rust
//! D8: AST normalization BEFORE lowering — (a) `::use` → the component body
//! inlined as real `Node`s with each `@param` bound (recursive; acyclic per
//! the checker's E-COMPONENT-CYCLE); (b) `<choice persist="run" …>` → a
//! synthesized trailing `::set` node (dsl §11.1.1: the sugar IS exactly a
//! `::set{run.<path> = <value>}` appended to the arm).
//!
//! Component-sourced regions are wrapped in `__component-begin`/`-end`
//! sentinel directives (reserved `__` prefix — the parser can never produce
//! them from source). The stage walker (Task 8) consumes them into
//! `source { component }` stamps; they emit no records.

use std::collections::BTreeMap;

use lute_check::meta::StateSchema;
use lute_check::ComponentSet;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::{
    Arm, Attr, AttrValue, CelKind, CelSlot, Choice, ClipNode, Directive, Document, Node, Set,
};

pub const COMPONENT_BEGIN: &str = "__component-begin";
pub const COMPONENT_END: &str = "__component-end";

/// Normalize the tree in place: no `::use` survives; persists are real `Set`s.
/// Total; failures (gate-proven unreachable) degrade to `E-COMPILE-COMPONENT`.
pub fn normalize_document(
    doc: &mut Document,
    components: &ComponentSet,
    schema: &StateSchema,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for shot in &mut doc.shots {
        normalize_nodes(&mut shot.body, components, schema, &mut diags);
    }
    diags
}

fn normalize_nodes(
    nodes: &mut Vec<Node>,
    components: &ComponentSet,
    schema: &StateSchema,
    diags: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < nodes.len() {
        let is_use = matches!(&nodes[i], Node::Directive(d) if d.tag == "use");
        if is_use {
            let d = match nodes.remove(i) {
                Node::Directive(d) => d,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            let spliced = expand_use(&d, components, schema, diags);
            let n = spliced.len();
            nodes.splice(i..i, spliced);
            i += n; // bodies were normalized recursively — skip past them
            continue;
        }
        match &mut nodes[i] {
            Node::Branch(b) => {
                for c in &mut b.choices {
                    synth_persist(c, schema);
                    normalize_nodes(&mut c.body, components, schema, diags);
                }
            }
            Node::Match(m) => {
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            normalize_nodes(body, components, schema, diags)
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// `::use{component="name" <arg>=…}` → `[begin, …bound body…, end]`.
fn expand_use(
    d: &Directive,
    components: &ComponentSet,
    schema: &StateSchema,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Node> {
    let name = d
        .attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        });
    let Some(def) = name.as_deref().and_then(|n| components.table.get(n)) else {
        // Gate-proven unreachable (E-COMPONENT-UNDECLARED); degrade.
        diags.push(Diagnostic {
            code: "E-COMPILE-COMPONENT".to_string(),
            severity: Severity::Error,
            message: "`::use` names no resolvable component (gate should have caught this)"
                .to_string(),
            span: d.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
        });
        return Vec::new();
    };
    let name = name.unwrap_or_default();
    let args: BTreeMap<String, AttrValue> = d
        .attrs
        .iter()
        .filter(|a| a.key != "component")
        .map(|a| (a.key.clone(), a.value.clone()))
        .collect();
    let mut body: Vec<Node> = def
        .body
        .shots
        .iter()
        .flat_map(|s| s.body.iter().cloned())
        .collect();
    bind_params(&mut body, &args, &def.params);
    // Nested `::use` in the body expands recursively (acyclic per checker).
    normalize_nodes(&mut body, components, schema, diags);

    let span = d.span;
    let begin = Node::Directive(Directive {
        tag: COMPONENT_BEGIN.to_string(),
        attrs: vec![Attr {
            key: "component".to_string(),
            value: AttrValue::Str(name),
            value_span: span,
            span,
        }],
        span,
    });
    let end = Node::Directive(Directive {
        tag: COMPONENT_END.to_string(),
        attrs: Vec::new(),
        span,
    });
    let mut out = Vec::with_capacity(body.len() + 2);
    out.push(begin);
    out.append(&mut body);
    out.push(end);
    out
}

/// Bind `@param` uses to `::use` args. A whole-slot `@param` attr value is
/// replaced VALUE-LEVEL (a string arg becomes a plain `Str` attr — what a
/// string-typed attr position needs); a `@param` inside a larger CEL is
/// substituted textually, typed by the param's declared [`Type`].
fn bind_params(nodes: &mut [Node], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for node in nodes {
        match node {
            Node::Line(l) => bind_attrs(&mut l.attrs, args, params),
            Node::Directive(d) => bind_attrs(&mut d.attrs, args, params),
            Node::Set(s) => bind_slot(&mut s.expr, args, params),
            Node::Branch(b) => {
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        bind_slot(w, args, params);
                    }
                    bind_attrs(&mut c.attrs, args, params);
                    bind_params(&mut c.body, args, params);
                }
            }
            Node::Match(m) => {
                bind_slot(&mut m.subject, args, params);
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            bind_slot(test, args, params);
                            bind_params(body, args, params);
                        }
                        Arm::Otherwise { body, .. } => bind_params(body, args, params),
                    }
                }
            }
            Node::Timeline(t) => {
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => bind_attrs(&mut d.attrs, args, params),
                            ClipNode::Set(s) => bind_slot(&mut s.expr, args, params),
                        }
                    }
                }
            }
        }
    }
}

fn bind_attrs(attrs: &mut [Attr], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for a in attrs {
        let AttrValue::Ref(slot) = &mut a.value else { continue };
        // Whole-slot `@param` → value-level replacement.
        if let Some(name) = slot.raw.trim().strip_prefix('@') {
            if let Some(arg) = args.get(name) {
                a.value = arg.clone();
                continue;
            }
        }
        bind_slot_raw(slot, args, params);
    }
}

fn bind_slot(slot: &mut CelSlot, args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    bind_slot_raw(slot, args, params);
}

/// Textual `@param` → arg substitution inside a CEL fragment (right-to-left).
fn bind_slot_raw(slot: &mut CelSlot, args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    let refs = lute_cel::scan_refs(&slot.raw);
    for r in refs.iter().rev() {
        if r.is_dollar || r.call.is_some() {
            continue; // params are 0-arity; calls/`$` belong to the expander
        }
        let Some(arg) = args.get(&r.name) else { continue };
        let ty = params.iter().find(|(n, _)| n == &r.name).map(|(_, t)| t);
        let text = arg_cel_text(arg, ty);
        slot.raw.replace_range(r.span.byte_start..r.span.byte_end, &text);
    }
}

fn arg_cel_text(arg: &AttrValue, ty: Option<&Type>) -> String {
    match arg {
        AttrValue::BoolTrue => "true".to_string(),
        AttrValue::Ref(slot) => slot.raw.clone(),
        AttrValue::Str(s) => match ty {
            Some(Type::Number) | Some(Type::Bool) => s.clone(),
            _ => cel_string_literal(s),
        },
    }
}

/// `<choice … persist="run" as="run.<path>" [value="<lit>"]>` → append
/// `Node::Set(run.<path> = <value>)` (dsl §11.1.1). Well-formedness is
/// gate-proven (E-PERSIST-*); anything unresolvable here is skipped, total.
fn synth_persist(choice: &mut Choice, schema: &StateSchema) {
    let find = |k: &str| choice.attrs.iter().find(|a| a.key == k);
    let persists = matches!(
        find("persist").map(|a| &a.value),
        Some(AttrValue::Str(s)) if s == "run"
    );
    if !persists {
        return;
    }
    let Some(AttrValue::Str(as_path)) = find("as").map(|a| &a.value) else {
        return; // gate: E-PERSIST-MISSING-AS
    };
    let as_path = as_path.clone();
    let Some(decl) = schema.decls.get(as_path.as_str()) else {
        return; // gate: E-PERSIST-TARGET
    };
    let value = find("value").and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.clone()),
        AttrValue::BoolTrue => Some("true".to_string()),
        AttrValue::Ref(_) => None, // gate: E-PERSIST-VALUE
    });
    let cel = persist_value_cel(&decl.ty, value.as_deref());
    let span = find("as").map(|a| a.span).unwrap_or(choice.span);
    push_set(choice, as_path, cel, span);
}

fn push_set(choice: &mut Choice, path: String, cel: String, span: Span) {
    choice.body.push(Node::Set(Set {
        path,
        path_span: span,
        op: "=".to_string(),
        expr: CelSlot::raw(CelKind::SetExpr, cel, span),
        span,
    }));
}

/// dsl §11.1.1 rule 4: bool target's value is optional (defaults `true`);
/// number stays bare; everything else (enum/str) is a CEL string literal.
fn persist_value_cel(ty: &Type, value: Option<&str>) -> String {
    match ty {
        Type::Bool => value.unwrap_or("true").to_string(),
        Type::Number => value.unwrap_or("0").to_string(),
        _ => cel_string_literal(value.unwrap_or_default()),
    }
}

/// Quote `s` as a single-quoted CEL string literal (backslash escaping, §4.4).
pub fn cel_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}
```



- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-compile normalize`
Expected: PASS — 3 tests ok.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/normalize.rs crates/lute-compile/src/lib.rs
git commit -m "feat(compile): AST normalization - ::use expansion + persist Set synthesis (D8)"
```

---

### Task 7: Direct lowering of primitive nodes (`lower.rs`, P5)

Pure, schema-driven per-primitive lowering (§5 pass 4): `:line` (role derivation), every core directive, `::set`, and the plugin passthrough. No addresses, no flow, no injection here — those are Tasks 8–11.

**Files:**
- Create: `crates/lute-compile/src/lower.rs` (implementation + in-module unit tests)
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod lower;`)

**Interfaces:**
- Consumes: `CapabilitySnapshot::directive(&self, tag: &str) -> Option<&DirectiveDecl>`; `DirectiveDecl { pub name: String, pub attrs: Vec<AttrDecl>, … }`, `AttrDecl { pub name: String, pub required: bool, pub ty: Type, pub default: Option<Literal> }`; `Literal::{Bool, Num, Str, …}`; AST `Line { pub speaker: String, pub attrs: Vec<Attr>, pub text: String, … }`, `Directive { pub tag: String, pub attrs: Vec<Attr>, pub span: Span }`, `Set { pub path: String, pub op: String, pub expr: CelSlot, … }`; Task 2's `Command` + kind structs; Task 6's `COMPONENT_BEGIN`/`COMPONENT_END`.
- Produces (Tasks 8–9 consume):

```rust
pub fn lower_line(line: &Line) -> Command                                       // always Command::Line
pub fn lower_set(set: &Set) -> Command                                          // always Command::Set
pub fn lower_directive(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<Command>  // None for use/sentinels
pub fn effective_wait(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<bool>
pub(crate) fn attr_string(attrs: &[Attr], key: &str) -> Option<String>
```

- [ ] **Step 1: Write the failing tests**

Create `crates/lute-compile/src/lower.rs` with ONLY the test module first:

```rust
#[cfg(test)]
mod tests {
    use lute_core_span::Severity;
    use lute_manifest::snapshot::CapabilitySnapshot;
    use lute_syntax::ast::Node;

    use super::*;

    fn nodes(body: &str) -> Vec<Node> {
        let src = format!(
            "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n"
        );
        let (doc, diags) = lute_syntax::parse(&src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc.shots[0].body.clone()
    }

    fn snap() -> CapabilitySnapshot {
        lute_manifest::core::load_core_snapshot()
    }

    fn lower_first(body: &str) -> serde_json::Value {
        let ns = nodes(body);
        let cmd = match &ns[0] {
            Node::Line(l) => lower_line(l),
            Node::Directive(d) => lower_directive(d, &snap()).expect("lowers"),
            Node::Set(s) => lower_set(s),
            other => panic!("unexpected node {other:?}"),
        };
        serde_json::to_value(&cmd).unwrap()
    }

    #[test]
    fn line_roles_derive_from_speaker_and_delivery() {
        let v = lower_first(":line[narrator]: Venny's.");
        assert_eq!(v["kind"], "line");
        assert_eq!(v["role"], "narration");
        let v = lower_first(":line[fixer]{delivery=\"thought\"}: Hm.");
        assert_eq!(v["role"], "monologue");
        let v = lower_first(":line[fixer]{delivery=\"voiceover\"}: Later.");
        assert_eq!(v["role"], "voiceover");
        let v = lower_first(
            ":line[bianca]{code=\"0010\" emotion=\"surprised\" variant=\"0\" as=\"Hostess\"}: Oh!",
        );
        assert_eq!(v["role"], "dialogue");
        assert_eq!(v["speaker"], "bianca");
        assert_eq!(v["text"], "Oh!");
        assert_eq!(v["emotion"], "surprised");
        assert_eq!(v["variant"], 0);
        assert_eq!(v["as"], "Hostess");
        // `code` is consumed into identity later — never a JSON field.
        assert!(v.get("code").is_none());
        // `delivery` is consumed into `role`.
        assert!(v.get("delivery").is_none());
    }

    #[test]
    fn bg_defaults_wait_true_camera_defaults_wait_false() {
        let v = lower_first("::bg{location=\"family_restaurant\" time=\"afternoon\" assetId=\"BG.x\"}");
        assert_eq!(v["kind"], "background");
        assert_eq!(v["location"], "family_restaurant");
        assert_eq!(v["time"], "afternoon");
        assert_eq!(v["assetId"], "BG.x");
        assert_eq!(v["wait"], true);
        let v = lower_first(
            "::camera{focus=\"bianca\" zoom=\"1.1\" move-x=\"0.2\" duration=\"0.5\" easing=\"ease-out\"}",
        );
        assert_eq!(v["kind"], "camera");
        assert_eq!(v["zoom"], 1.1);
        assert_eq!(v["moveX"], 0.2);
        assert_eq!(v["duration"], 0.5);
        assert_eq!(v["easing"], "ease-out");
        assert_eq!(v["wait"], false); // manifest default (arch §1 open question)
        let v = lower_first("::camera{shake=\"0.6\" wait=\"true\"}");
        assert_eq!(v["wait"], true); // author override beats the default
    }

    #[test]
    fn remaining_core_directives_lower_to_their_kinds() {
        let v = lower_first("::music{action=\"start\" mood=\"peaceful\" volume=\"down\" assetId=\"m.mp3\"}");
        assert_eq!(v["kind"], "music");
        assert_eq!(v["action"], "start");
        assert_eq!(v["mood"], "peaceful");
        assert_eq!(v["volume"], "down");
        let v = lower_first("::sfx{sound=\"hum\" assetId=\"s.mp3\"}");
        assert_eq!(v["kind"], "sfx");
        assert_eq!(v["sound"], "hum");
        let v = lower_first("::vfx{type=\"whiteOut\" transition=\"flash\"}");
        assert_eq!(v["kind"], "vfx");
        assert_eq!(v["vfxType"], "whiteOut");
        let v = lower_first("::cut{assetId=\"CUT.x\" full}");
        assert_eq!(v["kind"], "cut");
        assert_eq!(v["assetId"], "CUT.x");
        assert_eq!(v["full"], true);
        let v = lower_first("::video{assetId=\"MOVIE.x\" action=\"show\"}");
        assert_eq!(v["kind"], "video");
        assert_eq!(v["wait"], true);
        let v = lower_first("::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}");
        assert_eq!(v["kind"], "sprite");
        assert_eq!(v["character"], "bianca");
        assert_eq!(v["anchor"], "center");
        assert!(v.get("exit").is_none());
        let v = lower_first("::auto{character=\"bianca\" action=\"fade-out-down\"}");
        assert_eq!(v["exit"], true);
    }

    #[test]
    fn set_ops_lower_verbatim() {
        for op in ["=", "+=", "-=", "*="] {
            let v = lower_first(&format!("::set{{scene.affect.bianca {op} 1}}"));
            assert_eq!(v["kind"], "set");
            assert_eq!(v["path"], "scene.affect.bianca");
            assert_eq!(v["op"], *op);
            assert_eq!(v["value"], "1");
        }
    }

    #[test]
    fn plugin_directive_passes_through_with_typed_fields() {
        // `::minigame` is NOT in the core snapshot => generic passthrough
        // (plan spec-gap note 1); untyped attrs stay strings.
        let v = lower_first("::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\"}");
        assert_eq!(v["kind"], "plugin");
        assert_eq!(v["tag"], "minigame");
        assert_eq!(v["fields"]["kind"], "rhythm");
        assert_eq!(v["fields"]["resultKey"], "service01");
    }

    #[test]
    fn use_and_sentinels_lower_to_nothing() {
        let ns = nodes("::use{component=\"greet\" who=\"bianca\"}");
        let Node::Directive(d) = &ns[0] else { panic!() };
        assert!(lower_directive(d, &snap()).is_none());
        let begin = lute_syntax::ast::Directive {
            tag: crate::normalize::COMPONENT_BEGIN.to_string(),
            attrs: Vec::new(),
            span: d.span,
        };
        assert!(lower_directive(&begin, &snap()).is_none());
    }
}
```

Add `pub mod lower;` to `crates/lute-compile/src/lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-compile lower`
Expected: FAIL — `error[E0425]: cannot find function lower_line`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/lute-compile/src/lower.rs`:

```rust
//! Pass-1 direct lowering (§5): each primitive node → its typed record,
//! schema-driven and pure. `addr`/`lineId`/`voiceKey` stay empty here — the
//! addressing pass (Task 11) owns identity; the stage walker (Tasks 8–9)
//! owns order, stamps, and injection.

use std::collections::BTreeMap;

use lute_manifest::schema::DirectiveDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{Attr, AttrValue, Directive, Line, Set};

use crate::ir::*;
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

pub fn lower_line(line: &Line) -> Command {
    let get = |k: &str| attr_string(&line.attrs, k);
    let role = if line.speaker == "narrator" {
        Role::Narration
    } else {
        match get("delivery").as_deref() {
            Some("thought") => Role::Monologue,
            Some("voiceover") => Role::Voiceover,
            _ => Role::Dialogue,
        }
    };
    Command::Line(LineCmd {
        addr: String::new(),
        role,
        speaker: line.speaker.clone(),
        text: line.text.clone(),
        emotion: get("emotion"),
        variant: get("variant").and_then(|v| v.parse::<i64>().ok()),
        action: get("action"),
        dialog_motion: get("dialogMotion"),
        as_label: get("as"),
        line_id: String::new(),
        voice_key: None,
        code: get("code"),
        stamp: Stamp::default(),
    })
}

pub fn lower_set(set: &Set) -> Command {
    Command::Set(SetCmd {
        addr: String::new(),
        path: set.path.clone(),
        op: set.op.clone(),
        value: set.expr.raw.clone(),
        stamp: Stamp::default(),
    })
}

/// Lower one directive. `None` for `::use` and the component sentinels (the
/// walker consumes those); `Some(Command::Other(..))` for plugin directives.
pub fn lower_directive(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<Command> {
    let get = |k: &str| attr_string(&dir.attrs, k);
    let get_f64 = |k: &str| attr_f64(&dir.attrs, k);
    let get_bool = |k: &str| attr_bool(&dir.attrs, k);
    let stamp = Stamp {
        wait: effective_wait(dir, snapshot),
        duration: get_f64("duration"),
        delay: get_f64("delay"),
        ..Stamp::default()
    };
    Some(match dir.tag.as_str() {
        "bg" => Command::Background(BackgroundCmd {
            addr: String::new(),
            location: get("location"),
            time: get("time"),
            asset_id: get("assetId"),
            stamp,
        }),
        "music" => Command::Music(MusicCmd {
            addr: String::new(),
            action: get("action").unwrap_or_default(),
            mood: get("mood"),
            volume: get("volume"),
            asset_id: get("assetId"),
            track: get("track"),
            stamp,
        }),
        "sfx" => Command::Sfx(SfxCmd {
            addr: String::new(),
            sound: get("sound"),
            asset_id: get("assetId"),
            name: get("name"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            addr: String::new(),
            vfx_type: get("type").unwrap_or_default(),
            label: get("label"),
            transition: get("transition"),
            stamp,
        }),
        "auto" => {
            let action = get("action");
            let exit = match action.as_deref() {
                Some(a) if is_exit_action(a) => Some(true),
                _ => None,
            };
            Command::Sprite(SpriteCmd {
                addr: String::new(),
                character: get("character").unwrap_or_default(),
                anchor: get("anchor"),
                action,
                exit,
                pos_reset: None,
                preload: None,
                emotion: None,
                stamp,
            })
        }
        "camera" => Command::Camera(CameraCmd {
            addr: String::new(),
            focus: get("focus"),
            zoom: get_f64("zoom"),
            move_x: get_f64("move-x"),
            move_y: get_f64("move-y"),
            shake: get("shake"),
            reset: get_bool("reset"),
            easing: get("easing"),
            stamp,
        }),
        "cut" => Command::Cut(CutCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            full: get_bool("full"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            stamp,
        }),
        "use" | COMPONENT_BEGIN | COMPONENT_END => return None,
        _ => {
            // Plugin passthrough (plan spec-gap note 1): fields typed via the
            // directive's manifest AttrDecls when the decl is known.
            let decl = snapshot.directive(&dir.tag);
            let mut fields = BTreeMap::new();
            for a in &dir.attrs {
                if a.key == "wait" || a.key == "duration" || a.key == "delay" {
                    continue; // already resolved into the stamp
                }
                fields.insert(a.key.clone(), attr_json(a, decl));
            }
            Command::Other(OtherCmd {
                addr: String::new(),
                tag: dir.tag.clone(),
                fields,
                stamp,
            })
        }
    })
}

/// Resolved effective blocking (§4.3): author `wait` attr → manifest
/// `AttrDecl.default` → builtin fallback (`bg`/`video` block by default,
/// §4.4; everything else emits no `wait`).
pub fn effective_wait(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<bool> {
    if let Some(b) = attr_bool(&dir.attrs, "wait") {
        return Some(b);
    }
    if let Some(decl) = snapshot.directive(&dir.tag) {
        if let Some(a) = decl.attrs.iter().find(|a| a.name == "wait") {
            if let Some(Literal::Bool(b)) = &a.default {
                return Some(*b);
            }
        }
    }
    match dir.tag.as_str() {
        "bg" | "video" => Some(true),
        _ => None,
    }
}

/// dsl Appendix A `::auto` exit vocabulary (mirrors `lute-check::inject`'s
/// private helper byte-for-byte).
fn is_exit_action(action: &str) -> bool {
    action.starts_with("fade-out") || action.starts_with("exit") || action == "hide"
}

pub(crate) fn attr_string(attrs: &[Attr], key: &str) -> Option<String> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.clone()),
        AttrValue::Ref(slot) => Some(slot.raw.clone()),
        AttrValue::BoolTrue => Some("true".to_string()),
    })
}

fn attr_f64(attrs: &[Attr], key: &str) -> Option<f64> {
    attr_string(attrs, key).and_then(|s| s.parse::<f64>().ok())
}

fn attr_bool(attrs: &[Attr], key: &str) -> Option<bool> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::BoolTrue => Some(true),
        AttrValue::Str(s) => match s.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        AttrValue::Ref(_) => None,
    })
}

fn attr_json(attr: &Attr, decl: Option<&DirectiveDecl>) -> serde_json::Value {
    let ty = decl
        .and_then(|d| d.attrs.iter().find(|a| a.name == attr.key))
        .map(|a| &a.ty);
    match &attr.value {
        AttrValue::BoolTrue => serde_json::Value::Bool(true),
        AttrValue::Ref(slot) => serde_json::Value::String(slot.raw.clone()),
        AttrValue::Str(s) => match ty {
            Some(Type::Number) => s
                .parse::<f64>()
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or_else(|| serde_json::Value::String(s.clone())),
            Some(Type::Bool) => match s.as_str() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                _ => serde_json::Value::String(s.clone()),
            },
            _ => serde_json::Value::String(s.clone()),
        },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-compile lower`
Expected: PASS — 6 tests ok.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/lower.rs crates/lute-compile/src/lib.rs
git commit -m "feat(compile): direct lowering of primitive nodes (pass 1)"
```

---

### Task 8: Symbolic labels + branch/match flattening (`cfg.rs` + `stage.rs`, P6)

Flat-sequence flattening (D2, §7): `<branch>` → `choice` header + one arm subgraph per option + trailing `jump → converge` per arm + a convergence label; `<match>` → same with `arms`/`otherwise`. Targets are compiler-internal symbolic labels (`"@<n>"` strings) resolved to `addr`s in Task 11 — never serialized. The label mechanism (`Emitter::bind` attaches pending labels to the NEXT pushed record) is exactly what makes the §7.2 nesting rule fall out: an inner convergence label binds to the outer arm's trailing jump.

This task's `walk_seq` threads `StageState` through WITHOUT consuming it (returns it untouched; per-arm clones are discarded) — Task 9 adds injection and fork/join under the SAME signature, and this task's flatten tests keep passing unchanged. The `Node::Timeline` arm is a documented no-op that Task 10 replaces.

**Files:**
- Create: `crates/lute-compile/src/cfg.rs`
- Create: `crates/lute-compile/src/stage.rs`
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod cfg;` + `pub mod stage;`)
- Test: `crates/lute-compile/tests/flatten.rs`

**Interfaces:**
- Consumes: Task 2 IR; Task 7 `lower_line`/`lower_directive`/`lower_set`; Task 6 sentinels; `lute_check::StageState` (`Clone + Default`; fields `on_stage: BTreeMap<String, SpriteState>`, `dirty: BTreeSet<String>`, `bg: Option<String>`, `music: Option<String>`, `diags: Vec<Diagnostic>`); `lute_check::ctx::Env`.
- Produces (Tasks 9–12 consume):

```rust
// cfg.rs
pub struct Label(pub u32);                       // Clone, Copy, PartialEq, Eq, Debug
impl Label { pub fn sym(self) -> String;         // "@<n>"
             pub fn parse_sym(s: &str) -> Option<u32>; }
pub struct Rec { pub labels: Vec<Label>, pub cmd: Command }
#[derive(Default)] pub struct Emitter { pub recs: Vec<Rec>, /* pending, next: private */ }
impl Emitter { pub fn fresh(&mut self) -> Label;
               pub fn bind(&mut self, l: Label);
               pub fn push(&mut self, cmd: Command);
               pub fn finish(self) -> (Vec<Rec>, Vec<Label>); } // trailing labels
// stage.rs
pub struct WalkCx<'a> { pub snapshot: &'a CapabilitySnapshot, pub env: &'a Env,
                        pub components: Vec<String>, pub timelines: u32 }
pub fn walk_seq(em: &mut Emitter, nodes: &[Node], state: StageState, cx: &mut WalkCx<'_>) -> StageState
```

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/flatten.rs`:

```rust
//! Flatten-shape goldens (§7): header/arm/jump/converge order, symbolic
//! label binding, the §7.2 nesting rule, empty arms, end-of-shot converges,
//! and component-sentinel consumption.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::cfg::{Emitter, Label, Rec};
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

fn flatten(body: &str) -> (Vec<Rec>, Vec<Label>) {
    let src = format!(
        "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n"
    );
    let (doc, diags) = lute_syntax::parse(&src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let env = Env::default();
    let mut cx = WalkCx {
        snapshot: &snapshot,
        env: &env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut em = Emitter::default();
    let _ = walk_seq(&mut em, &doc.shots[0].body, StageState::default(), &mut cx);
    em.finish()
}

fn kind(cmd: &Command) -> &'static str {
    match cmd {
        Command::Line(_) => "line",
        Command::Background(_) => "background",
        Command::Music(_) => "music",
        Command::Sfx(_) => "sfx",
        Command::Vfx(_) => "vfx",
        Command::Sprite(_) => "sprite",
        Command::Camera(_) => "camera",
        Command::Cut(_) => "cut",
        Command::Video(_) => "video",
        Command::Set(_) => "set",
        Command::Choice(_) => "choice",
        Command::Match(_) => "match",
        Command::Jump(_) => "jump",
        Command::Barrier(_) => "barrier",
        Command::Other(_) => "plugin",
    }
}

const BRANCH: &str = r#"<branch id="number">
  <choice id="blunt" label="Just ask, flatly">
    :line[fixer]{code="0050"}: Bianca. Your number.
  </choice>
  <choice id="soft" label="Ask gently">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>
:line[narrator]: She answers."#;

#[test]
fn branch_flattens_to_header_arms_jumps_converge() {
    let (recs, trailing) = flatten(BRANCH);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["choice", "line", "jump", "set", "jump", "line"]);
    assert!(trailing.is_empty());

    let Command::Choice(c) = &recs[0].cmd else { panic!() };
    assert_eq!(c.branch_id, "number");
    assert_eq!(c.record_key, "scene.choices.number");
    // Option targets point at each arm's first record's label.
    assert_eq!(c.options[0].target, recs[1].labels[0].sym());
    assert_eq!(c.options[1].target, recs[3].labels[0].sym());
    // Converge label binds on the narrator line after the block.
    assert_eq!(c.converge, recs[5].labels[0].sym());
    // Both arm-trailing jumps return to the converge.
    for i in [2usize, 4] {
        let Command::Jump(j) = &recs[i].cmd else { panic!() };
        assert_eq!(j.target, c.converge);
    }
}

#[test]
fn match_flattens_with_otherwise_and_omits_it_when_absent() {
    let m = r#"<match on="scene.flags.saw_beam">
  <when test="$ == true">
    :line[fixer]{delivery="thought"}: saw
  </when>
  <otherwise>
    :line[fixer]{delivery="thought"}: not
  </otherwise>
</match>
:line[narrator]: on."#;
    let (recs, _) = flatten(m);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["match", "line", "jump", "line", "jump", "line"]);
    let Command::Match(mc) = &recs[0].cmd else { panic!() };
    assert_eq!(mc.subject, "scene.flags.saw_beam");
    assert_eq!(mc.arms.len(), 1);
    assert_eq!(mc.arms[0].target, recs[1].labels[0].sym());
    assert_eq!(mc.otherwise.as_deref(), Some(recs[3].labels[0].sym().as_str()));
    assert_eq!(mc.converge, recs[5].labels[0].sym());

    // No <otherwise> arm (gate-proven covered) => field omitted (§11.2).
    let covered = r#"<match on="scene.flags.saw_beam">
  <when test="$ == true">
    :line[fixer]{delivery="thought"}: t
  </when>
  <when test="$ == false">
    :line[fixer]{delivery="thought"}: f
  </when>
</match>
:line[narrator]: on."#;
    let (recs, _) = flatten(covered);
    let Command::Match(mc) = &recs[0].cmd else { panic!() };
    assert!(mc.otherwise.is_none());
}

#[test]
fn nested_block_lays_inner_convergence_before_outer_jump() {
    let nested = r#"<branch id="outer">
  <choice id="a" label="A">
    <match on="scene.flags.saw_beam">
      <when test="$ == true">
        :line[fixer]{delivery="thought"}: saw
      </when>
      <otherwise>
        :line[fixer]{delivery="thought"}: not
      </otherwise>
    </match>
  </choice>
  <choice id="b" label="B">
    :line[fixer]{code="0010"}: b
  </choice>
</branch>
:line[narrator]: end."#;
    let (recs, _) = flatten(nested);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(
        kinds,
        vec!["choice", "match", "line", "jump", "line", "jump", "jump", "line", "jump", "line"]
    );
    // §7.2: the INNER convergence label binds on the OUTER arm-a trailing jump
    // (recs[6]), so control reaches the outer converge through it.
    let Command::Match(mc) = &recs[1].cmd else { panic!() };
    assert_eq!(mc.converge, recs[6].labels[0].sym());
    let Command::Choice(c) = &recs[0].cmd else { panic!() };
    let Command::Jump(outer_a) = &recs[6].cmd else { panic!() };
    assert_eq!(outer_a.target, c.converge);
    // Inner arm jumps return to the inner converge, not the outer one.
    for i in [3usize, 5] {
        let Command::Jump(j) = &recs[i].cmd else { panic!() };
        assert_eq!(j.target, mc.converge);
    }
    assert_eq!(c.converge, recs[9].labels[0].sym());
}

#[test]
fn empty_arm_is_a_bare_labeled_jump_and_last_block_converges_past_end() {
    let b = r#"<branch id="tail">
  <choice id="go" label="Go">
  </choice>
</branch>"#;
    let (recs, trailing) = flatten(b);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["choice", "jump"]);
    let Command::Choice(c) = &recs[0].cmd else { panic!() };
    // Empty arm: the arm label sits ON the bare jump (§7.2).
    assert_eq!(c.options[0].target, recs[1].labels[0].sym());
    // Branch is the LAST node: converge label is left trailing for Task 11's
    // one-past-end addr (plan spec-gap note 2).
    assert_eq!(trailing.len(), 1);
    assert_eq!(c.converge, trailing[0].sym());
}

#[test]
fn component_sentinels_stamp_source_and_emit_nothing() {
    let src = r#"::__component-begin{component="greet"}
::auto{character="bianca" anchor="center" action="fade-in-up"}
::__component-end
:line[narrator]: after."#;
    let (recs, _) = flatten(src);
    let kinds: Vec<_> = recs.iter().map(|r| kind(&r.cmd)).collect();
    assert_eq!(kinds, vec!["sprite", "line"]);
    let Command::Sprite(s) = &recs[0].cmd else { panic!() };
    assert_eq!(
        s.stamp.source.as_ref().map(|s| s.component.as_str()),
        Some("greet")
    );
    let Command::Line(l) = &recs[1].cmd else { panic!() };
    assert!(l.stamp.source.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test flatten`
Expected: FAIL — `error[E0432]: unresolved import lute_compile::cfg`.

- [ ] **Step 3: Write minimal implementation**

Create `crates/lute-compile/src/cfg.rs`:

```rust
//! Symbolic-label machinery for branch/match flattening (§7). A [`Label`] is
//! a compiler-internal temporary: flattening writes `"@<n>"` into target
//! fields, [`Emitter::bind`] parks a label on the NEXT pushed record, and the
//! addressing pass (Task 11) rewrites every `"@<n>"` to a concrete `addr` —
//! labels are never serialized.

use crate::ir::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label(pub u32);

impl Label {
    /// Symbolic target text: `"@<n>"` — cannot collide with a real addr
    /// (`"{shot:03}-{idx:04}"`).
    pub fn sym(self) -> String {
        format!("@{}", self.0)
    }

    /// Parse a symbolic target back to its label number.
    pub fn parse_sym(s: &str) -> Option<u32> {
        s.strip_prefix('@').and_then(|n| n.parse().ok())
    }
}

/// One emitted record plus the labels bound AT it (its future `addr` is the
/// labels' resolution).
#[derive(Clone, Debug)]
pub struct Rec {
    pub labels: Vec<Label>,
    pub cmd: Command,
}

/// Per-shot record emitter (labels never cross shots).
#[derive(Default)]
pub struct Emitter {
    pub recs: Vec<Rec>,
    pending: Vec<Label>,
    next: u32,
}

impl Emitter {
    pub fn fresh(&mut self) -> Label {
        let l = Label(self.next);
        self.next += 1;
        l
    }

    /// Park `l` to bind on the next pushed record (or trail past the end).
    pub fn bind(&mut self, l: Label) {
        self.pending.push(l);
    }

    pub fn push(&mut self, cmd: Command) {
        let labels = std::mem::take(&mut self.pending);
        self.recs.push(Rec { labels, cmd });
    }

    /// The records plus any labels still pending past the last record (an
    /// end-of-shot convergence, plan spec-gap note 2).
    pub fn finish(self) -> (Vec<Rec>, Vec<Label>) {
        (self.recs, self.pending)
    }
}
```

Create `crates/lute-compile/src/stage.rs`:

```rust
//! The document walker: flatten (this task) + CFG-aware stage resolution
//! (Task 9, D9) + inline timelines (Task 10). ONE walk owns emission order.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, AttrValue, Branch, Directive, Match, Node};

use crate::cfg::{Emitter, Label};
use crate::ir::*;
use crate::lower::{lower_directive, lower_line, lower_set};
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

/// Walk context: the read-only capability surface + the component-source
/// stack (sentinel-driven) + the document-order timeline counter (Task 10).
pub struct WalkCx<'a> {
    pub snapshot: &'a CapabilitySnapshot,
    pub env: &'a Env,
    pub components: Vec<String>,
    pub timelines: u32,
}

/// Timeline-clip stamp for records emitted inside a `<timeline>` (Task 10).
#[derive(Clone, Copy)]
pub struct ClipStamp {
    pub timeline: u32,
    pub at: f64,
    pub duration: f64,
}

/// Walk one node sequence in document order, emitting records into `em` and
/// threading `StageState` (identity in this task; injection lands in Task 9).
pub fn walk_seq(
    em: &mut Emitter,
    nodes: &[Node],
    mut state: StageState,
    cx: &mut WalkCx<'_>,
) -> StageState {
    for (i, node) in nodes.iter().enumerate() {
        match node {
            Node::Directive(d) if d.tag == COMPONENT_BEGIN => {
                cx.components.push(component_attr(d));
            }
            Node::Directive(d) if d.tag == COMPONENT_END => {
                cx.components.pop();
            }
            Node::Line(_) | Node::Directive(_) | Node::Set(_) => {
                state = emit_primitive(em, node, state, lookahead(nodes, i), cx, None);
            }
            Node::Branch(b) => {
                state = walk_branch(em, b, state, cx);
            }
            Node::Match(m) => {
                state = walk_match(em, m, state, cx);
            }
            Node::Timeline(_) => {
                // Replaced in Task 10 (schedule.rs): a timeline is handled
                // INLINE in this same walk (§5 pass 5).
            }
        }
    }
    state
}

/// D9 lookahead: only CFG-reachable LINEAR successors — the rest of this
/// sequence up to (never into) the next fork. Sibling arms are unreachable.
fn lookahead(nodes: &[Node], i: usize) -> &[Node] {
    let rest = &nodes[i + 1..];
    let stop = rest
        .iter()
        .position(|n| matches!(n, Node::Branch(_) | Node::Match(_)))
        .unwrap_or(rest.len());
    &rest[..stop]
}

/// Lower one primitive node into records. Task 9 adds injection here; this
/// task emits the authored record only and passes the state through.
fn emit_primitive(
    em: &mut Emitter,
    node: &Node,
    state: StageState,
    _lookahead: &[Node],
    cx: &mut WalkCx<'_>,
    clip: Option<ClipStamp>,
) -> StageState {
    let authored = match node {
        Node::Line(l) => Some(lower_line(l)),
        Node::Directive(d) => lower_directive(d, cx.snapshot),
        Node::Set(s) => Some(lower_set(s)),
        _ => None,
    };
    if let Some(mut cmd) = authored {
        apply_source(&mut cmd, cx);
        apply_clip(&mut cmd, clip);
        em.push(cmd);
    }
    state
}

fn walk_branch(em: &mut Emitter, b: &Branch, state: StageState, cx: &mut WalkCx<'_>) -> StageState {
    let conv = em.fresh();
    let arms: Vec<Label> = b.choices.iter().map(|_| em.fresh()).collect();
    let options = b
        .choices
        .iter()
        .zip(&arms)
        .map(|(c, l)| ChoiceOption {
            id: c.id.clone(),
            label: c.label.clone(),
            line_id: String::new(),
            when: c.when.as_ref().map(|w| w.raw.clone()),
            target: l.sym(),
        })
        .collect();
    let mut cmd = Command::Choice(ChoiceCmd {
        addr: String::new(),
        branch_id: b.id.clone(),
        record_key: format!("scene.choices.{}", b.id),
        options,
        converge: conv.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd);
    for (c, l) in b.choices.iter().zip(&arms) {
        em.bind(*l);
        // Task 9 forks/joins here; flatten-only for now.
        let _ = walk_seq(em, &c.body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
    }
    em.bind(conv);
    state
}

fn walk_match(em: &mut Emitter, m: &Match, state: StageState, cx: &mut WalkCx<'_>) -> StageState {
    let conv = em.fresh();
    let labels: Vec<Label> = m.arms.iter().map(|_| em.fresh()).collect();
    let mut arms = Vec::new();
    let mut otherwise = None;
    for (arm, l) in m.arms.iter().zip(&labels) {
        match arm {
            Arm::When { test, .. } => arms.push(MatchArm {
                test: test.raw.clone(),
                target: l.sym(),
            }),
            Arm::Otherwise { .. } => otherwise = Some(l.sym()),
        }
    }
    let mut cmd = Command::Match(MatchCmd {
        addr: String::new(),
        subject: m.subject.raw.clone(),
        arms,
        otherwise,
        converge: conv.sym(),
        stamp: Stamp::default(),
    });
    apply_source(&mut cmd, cx);
    em.push(cmd);
    for (arm, l) in m.arms.iter().zip(&labels) {
        let body = match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
        };
        em.bind(*l);
        let _ = walk_seq(em, body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
    }
    em.bind(conv);
    state
}

/// `source { component }` from the sentinel-driven stack (§4.3, D8).
fn apply_source(cmd: &mut Command, cx: &WalkCx<'_>) {
    if let Some(name) = cx.components.last() {
        if let Some(stamp) = cmd.stamp_mut() {
            stamp.source = Some(Source {
                component: name.clone(),
            });
        }
    }
}

/// `timeline`/`at`/`duration` stamps on timeline-clip records (§4.3, Task 10).
fn apply_clip(cmd: &mut Command, clip: Option<ClipStamp>) {
    let Some(c) = clip else { return };
    if let Some(stamp) = cmd.stamp_mut() {
        stamp.timeline = Some(c.timeline);
        stamp.at = Some(c.at);
        if c.duration > 0.0 {
            stamp.duration = Some(c.duration);
        }
    }
}

fn component_attr(d: &Directive) -> String {
    d.attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}
```

Add to `crates/lute-compile/src/lib.rs`: `pub mod cfg;` and `pub mod stage;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile --test flatten`
Expected: PASS — 5 tests ok.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/cfg.rs crates/lute-compile/src/stage.rs crates/lute-compile/src/lib.rs crates/lute-compile/tests/flatten.rs
git commit -m "feat(compile): branch/match flattening over symbolic labels (D2)"
```

---

### Task 9: CFG-aware stage resolution — injection + fork/join (`stage.rs`, P7a / D9)

Thread `lute_check::inject::lower_node` through the SAME walk: injected commands become SEPARATE `sprite` records with provenance (§7.4); each branch/match arm forks a clone of the entry `StageState` and the exits JOIN conservatively at the convergence (§7.3 — plan spec-gap note 7: a differing/partial slot is dropped, encoding `Unknown`). Placement (plan spec-gap note 4): `::auto` → authored record first, then its injections; every other node → injections first.

**Files:**
- Modify: `crates/lute-compile/src/stage.rs` (extend `emit_primitive`, `walk_branch`, `walk_match`; add `inject_cmd`, `join_states`)
- Test: `crates/lute-compile/tests/inject.rs`

**Interfaces:**
- Consumes: `lute_check::lower_node(state: StageState, node: &Node, lookahead: &[Node]) -> (StageState, Vec<InjectedCommand>)`; `InjectedCommand { pub kind: InjectKind, pub provenance: Provenance }`; `InjectKind::{Anchor { character, anchor }, PosReset { character }, SpriteLoad { character, emotion }, Hide { character }}`; `SpriteState { pub anchor: Option<String>, pub pose: Option<String>, pub emotion: Option<String> }` (PartialEq); Task 8's walker.
- Produces: `walk_seq` unchanged signature, now injection-aware; `pub fn join_states(entry: &StageState, exits: Vec<StageState>) -> StageState` (unit-tested; Task 10 relies on the same `emit_primitive`).

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/inject.rs`:

```rust
//! Injection goldens (arch #10): the four rules as SEPARATE sprite records
//! with provenance, placement relative to the authored record, and the D9
//! branch fork/join case.

use lute_check::ctx::Env;
use lute_check::{SpriteState, StageState};
use lute_compile::cfg::{Emitter, Rec};
use lute_compile::stage::{join_states, walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

fn walk(body: &str) -> (Vec<Rec>, StageState) {
    let src = format!(
        "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n"
    );
    let (doc, diags) = lute_syntax::parse(&src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let env = Env::default();
    let mut cx = WalkCx {
        snapshot: &snapshot,
        env: &env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut em = Emitter::default();
    let state = walk_seq(&mut em, &doc.shots[0].body, StageState::default(), &mut cx);
    let (recs, _) = em.finish();
    (recs, state)
}

fn sprite_desc(cmd: &Command) -> Option<String> {
    let Command::Sprite(s) = cmd else { return None };
    let by = s
        .stamp
        .provenance
        .as_ref()
        .map(|p| p.by.as_str())
        .unwrap_or("authored");
    let what = if s.pos_reset == Some(true) {
        "posReset"
    } else if s.preload == Some(true) {
        "preload"
    } else if s.exit == Some(true) {
        "exit"
    } else if s.anchor.is_some() && s.action.is_none() && s.stamp.provenance.is_some() {
        "anchor"
    } else {
        "show"
    };
    Some(format!("{}:{}:{}", s.character, what, by))
}

#[test]
fn anchor_and_preload_inject_after_authored_auto() {
    // No explicit anchor + a first emotion ahead => auto-anchor-on-show and
    // entry-emotion-lookahead, each a separate record AFTER the authored
    // sprite (§4.5 worked example).
    let (recs, _) = walk(
        "::auto{character=\"bianca\" action=\"fade-in-up\"}\n:line[bianca]{emotion=\"surprised\"}: Oh!",
    );
    let sprites: Vec<String> = recs.iter().filter_map(|r| sprite_desc(&r.cmd)).collect();
    assert_eq!(
        sprites,
        vec![
            "bianca:show:authored".to_string(),
            "bianca:anchor:auto-anchor-on-show".to_string(),
            "bianca:preload:entry-emotion-lookahead".to_string(),
        ]
    );
    // The injected anchor record carries the default anchor.
    let Command::Sprite(anchor) = &recs[1].cmd else { panic!() };
    assert_eq!(anchor.anchor.as_deref(), Some("center"));
    assert_eq!(
        anchor.stamp.provenance.as_ref().map(|p| p.injected),
        Some(true)
    );
}

#[test]
fn pos_reset_injects_before_the_plain_line() {
    let body = "::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}\n\
:line[bianca]{emotion=\"delighted\" action=\"pose-lean\"}: A!\n\
:line[bianca]: B.";
    let (recs, _) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.pos_reset == Some(true) => "posReset",
            Command::Sprite(_) => "sprite",
            Command::Line(_) => "line",
            _ => "other",
        })
        .collect();
    // preload for the stateful first line, then: line A, posReset BEFORE line B.
    assert_eq!(kinds, vec!["sprite", "sprite", "line", "posReset", "line"]);
    let Command::Sprite(pr) = &recs[3].cmd else { panic!() };
    assert_eq!(
        pr.stamp.provenance.as_ref().map(|p| p.by.as_str()),
        Some("auto-pose-reset")
    );
}

#[test]
fn scene_change_hides_lingering_sprites_before_the_bg() {
    let body = "::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}\n\
::bg{location=\"street\" time=\"evening\"}";
    let (recs, state) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.exit == Some(true) => "hide",
            Command::Sprite(_) => "sprite",
            Command::Background(_) => "background",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["sprite", "hide", "background"]);
    let Command::Sprite(h) = &recs[1].cmd else { panic!() };
    assert_eq!(
        h.stamp.provenance.as_ref().map(|p| p.by.as_str()),
        Some("stage-bookkeeping")
    );
    assert!(state.on_stage.is_empty(), "stage cleared after scene change");
}

#[test]
fn branch_arms_fork_from_entry_state_and_join_conservatively() {
    // D9 fork/join golden (spec §8): BOTH arms show bianca fresh (each gets
    // its own anchor injection — nothing leaks from arm 1 into arm 2), and
    // the post-join ::auto is a fresh show again (differing arm emotions =>
    // the join drops bianca).
    let body = r#"<branch id="fork">
  <choice id="a" label="A">
    ::auto{character="bianca" action="fade-in-up"}
    :line[bianca]{emotion="surprised"}: Oh!
  </choice>
  <choice id="b" label="B">
    ::auto{character="bianca" action="fade-in-up"}
    :line[bianca]{emotion="delighted"}: Ha!
  </choice>
</branch>
::auto{character="bianca" action="fade-in-up"}"#;
    let (recs, _) = walk(body);
    let anchors: Vec<usize> = recs
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            matches!(sprite_desc(&r.cmd).as_deref(), Some(d) if d.contains(":anchor:"))
        })
        .map(|(i, _)| i)
        .collect();
    // Three anchor injections: one per arm + one after the join.
    assert_eq!(anchors.len(), 3, "{recs:#?}");
}

#[test]
fn join_states_unit_semantics() {
    let sprite = |emotion: &str| SpriteState {
        anchor: Some("center".to_string()),
        pose: None,
        emotion: Some(emotion.to_string()),
    };
    let mut a = StageState::default();
    a.on_stage.insert("bianca".into(), sprite("surprised"));
    a.on_stage.insert("takeru".into(), sprite("neutral"));
    a.dirty.insert("takeru".into());
    a.bg = Some("street".into());
    let mut b = StageState::default();
    b.on_stage.insert("bianca".into(), sprite("delighted"));
    b.on_stage.insert("takeru".into(), sprite("neutral"));
    b.dirty.insert("takeru".into());
    b.bg = Some("cafe".into());

    let joined = join_states(&StageState::default(), vec![a, b]);
    // Differing emotion => bianca dropped (Unknown, §7.3).
    assert!(!joined.on_stage.contains_key("bianca"));
    // Identical in every arm => carried, dirty intersection kept.
    assert!(joined.on_stage.contains_key("takeru"));
    assert!(joined.dirty.contains("takeru"));
    // Differing bg => Unknown (None).
    assert!(joined.bg.is_none());
    // Empty exits degrade to the entry state.
    let entry = StageState::default();
    assert!(join_states(&entry, Vec::new()).on_stage.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test inject`
Expected: FAIL — `error[E0432]: unresolved import ... join_states` (and, after a stub, missing injected records).

- [ ] **Step 3: Write minimal implementation**

In `crates/lute-compile/src/stage.rs`:

(a) extend the imports:

```rust
use lute_check::{lower_node, InjectKind, InjectedCommand, SpriteState};
```

(b) REPLACE `emit_primitive`'s body (same signature; the `_lookahead` param loses its underscore):

```rust
fn emit_primitive(
    em: &mut Emitter,
    node: &Node,
    state: StageState,
    lookahead: &[Node],
    cx: &mut WalkCx<'_>,
    clip: Option<ClipStamp>,
) -> StageState {
    // Pure reducer step (arch #2): next stage state + this node's injections.
    let (next, injected) = lower_node(state, node, lookahead);
    let authored = match node {
        Node::Line(l) => Some(lower_line(l)),
        Node::Directive(d) => lower_directive(d, cx.snapshot),
        Node::Set(s) => Some(lower_set(s)),
        _ => None,
    };
    // Placement (plan spec-gap note 4): an `::auto`'s injections (anchor,
    // preload) FOLLOW the authored show (§4.5); a line's posReset and a
    // scene-change's hides PRECEDE theirs.
    let auto_first = matches!(node, Node::Directive(d) if d.tag == "auto");
    if auto_first {
        if let Some(cmd) = authored {
            emit_stamped(em, cmd, cx, clip);
        }
        for ic in &injected {
            emit_stamped(em, inject_cmd(ic), cx, clip);
        }
    } else {
        for ic in &injected {
            emit_stamped(em, inject_cmd(ic), cx, clip);
        }
        if let Some(cmd) = authored {
            emit_stamped(em, cmd, cx, clip);
        }
    }
    next
}

fn emit_stamped(em: &mut Emitter, mut cmd: Command, cx: &WalkCx<'_>, clip: Option<ClipStamp>) {
    apply_source(&mut cmd, cx);
    apply_clip(&mut cmd, clip);
    em.push(cmd);
}

/// `InjectKind` → a SEPARATE `sprite` record with provenance (§7.4).
fn inject_cmd(ic: &InjectedCommand) -> Command {
    let stamp = Stamp {
        provenance: Some(ic.provenance.clone()),
        ..Stamp::default()
    };
    let sprite = |character: &str| SpriteCmd {
        addr: String::new(),
        character: character.to_string(),
        anchor: None,
        action: None,
        exit: None,
        pos_reset: None,
        preload: None,
        emotion: None,
        stamp,
    };
    Command::Sprite(match &ic.kind {
        InjectKind::Anchor { character, anchor } => SpriteCmd {
            anchor: Some(anchor.clone()),
            ..sprite(character)
        },
        InjectKind::PosReset { character } => SpriteCmd {
            pos_reset: Some(true),
            ..sprite(character)
        },
        InjectKind::SpriteLoad { character, emotion } => SpriteCmd {
            preload: Some(true),
            emotion: Some(emotion.clone()),
            ..sprite(character)
        },
        InjectKind::Hide { character } => SpriteCmd {
            exit: Some(true),
            ..sprite(character)
        },
    })
}
```

(c) in `walk_branch` and `walk_match`, replace the flatten-only arm loops + trailing `state` return with fork/join. `walk_branch`'s loop and tail become:

```rust
    // Fork (D9): every arm starts from the ENTRY state. Entry diagnostics are
    // drained first so per-arm clones don't duplicate them.
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let mut exits = Vec::with_capacity(b.choices.len());
    for (c, l) in b.choices.iter().zip(&arms) {
        em.bind(*l);
        let exit = walk_seq(em, &c.body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
        exits.push(exit);
    }
    em.bind(conv);
    let mut joined = join_states(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
    joined
```

(the fn signature keeps `state: StageState`; the block rebinds it via `let mut state = state;`). `walk_match`'s arm loop + tail become, correspondingly:

```rust
    let mut state = state;
    let base_diags = std::mem::take(&mut state.diags);
    let mut exits = Vec::with_capacity(m.arms.len());
    for (arm, l) in m.arms.iter().zip(&labels) {
        let body = match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
        };
        em.bind(*l);
        let exit = walk_seq(em, body, state.clone(), cx);
        em.push(Command::Jump(JumpCmd {
            addr: String::new(),
            target: conv.sym(),
        }));
        exits.push(exit);
    }
    em.bind(conv);
    let mut joined = join_states(&state, exits);
    let mut diags = base_diags;
    diags.append(&mut joined.diags);
    joined.diags = diags;
    joined
```

(d) add `join_states` (public — unit-tested and reused by Task 10's reasoning):

```rust
/// §7.3 conservative convergence join. Per character: identical `SpriteState`
/// in EVERY arm → carried; differing or partial → dropped (that encodes
/// `Unknown`: a later plain line assumes no pose — no false posReset — and a
/// later `::auto` is a fresh show → anchor + preload). `dirty` survives only
/// where carried AND dirty in every arm; `bg`/`music` carry only when
/// identical across arms. Exits' diagnostics concatenate in arm order.
pub fn join_states(entry: &StageState, mut exits: Vec<StageState>) -> StageState {
    let Some(first) = exits.first().cloned() else {
        return entry.clone();
    };
    let mut joined = StageState::default();
    for e in &mut exits {
        joined.diags.append(&mut e.diags);
    }
    'chars: for (ch, sprite) in &first.on_stage {
        for e in &exits[1..] {
            if e.on_stage.get(ch) != Some(sprite) {
                continue 'chars;
            }
        }
        joined.on_stage.insert(ch.clone(), sprite.clone());
    }
    let kept: Vec<String> = joined.on_stage.keys().cloned().collect();
    for ch in kept {
        if exits.iter().all(|e| e.dirty.contains(&ch)) {
            joined.dirty.insert(ch);
        }
    }
    joined.bg = if exits.iter().all(|e| e.bg == first.bg) {
        first.bg.clone()
    } else {
        None
    };
    joined.music = if exits.iter().all(|e| e.music == first.music) {
        first.music.clone()
    } else {
        None
    };
    joined
}
```

Note the unused-import check: `SpriteState` is only used by `join_states`'s doc semantics via `StageState` internals — if rustc flags it unused, drop it from the import list (the join compares through `BTreeMap::get`, which needs no named type).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-compile --test inject`
Expected: PASS — 5 tests ok.
Then the Task-8 regression guard: `cargo test -p lute-compile --test flatten`
Expected: PASS — flatten shapes unchanged (injection adds no records to those fixtures except where pinned).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/stage.rs crates/lute-compile/tests/inject.rs
git commit -m "feat(compile): CFG-aware stage resolution - injection + fork/join (D9)"
```

---

### Task 10: Inline timeline scheduling + barrier (`schedule.rs` + `stage.rs`, P7b)

A `<timeline>` is handled INLINE in the same CFG walk (§5 pass 5 — NOT a separate later pass): schedule clips via `lute-check::timeline` math (omitted-`at` cursor → absolute `at`), thread the resolved clips through the SAME reducer in deterministic `(at, track-order)` order (a clip MAY be a stage-changing `::auto`/`::bg`), stamp every emitted record with `timeline`+`at` (+`duration`), append a `barrier`, and carry the reducer's POST-BARRIER state to the next node.

**Files:**
- Create: `crates/lute-compile/src/schedule.rs`
- Modify: `crates/lute-compile/src/stage.rs` (replace the `Node::Timeline` no-op arm; add `walk_timeline`)
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod schedule;`)
- Test: `crates/lute-compile/tests/timeline.rs`

**Interfaces:**
- Consumes: `lute_check::resolve_timeline(tl: &Timeline, _ctx: &Ctx<'_>, snapshot: &CapabilitySnapshot) -> (ResolvedTimeline, Vec<Diagnostic>)` — `ResolvedTimeline { pub rows: Vec<ResolvedRow>, pub barrier_at: f64 }`, `ResolvedRow { pub at: f64, pub subject: String, pub summary: String, pub duration: f64 }`; rows are emitted one per clip, per track, in document order (verified in `timeline.rs:140-190`), so they zip 1:1 with `tl.tracks[*].clips[*]`; `Ctx { pub env: &'a Env, pub in_match: bool, pub match_subject: Option<String> }`; `ClipNode::{Directive, Set}`; Task 9's `emit_primitive` + `ClipStamp`.
- Produces (Task 12 consumes via `walk_seq`):

```rust
// schedule.rs
pub struct ScheduledClip<'a> { pub at: f64, pub duration: f64, pub track: usize, pub node: &'a ClipNode }
pub fn schedule_timeline<'a>(tl: &'a Timeline, ctx: &Ctx<'_>, snapshot: &CapabilitySnapshot) -> (Vec<ScheduledClip<'a>>, f64)
```

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/timeline.rs`:

```rust
//! Inline timeline goldens (§5 pass 5): deterministic (at, track) order,
//! timeline/at/duration stamps, the trailing barrier, a stage-changing clip
//! threading the reducer, and post-barrier state carry.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::cfg::{Emitter, Rec};
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

fn walk(body: &str) -> (Vec<Rec>, StageState) {
    let src = format!(
        "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n"
    );
    let (doc, diags) = lute_syntax::parse(&src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let env = Env::default();
    let mut cx = WalkCx {
        snapshot: &snapshot,
        env: &env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut em = Emitter::default();
    let state = walk_seq(&mut em, &doc.shots[0].body, StageState::default(), &mut cx);
    let (recs, _) = em.finish();
    (recs, state)
}

// The bianca-s01ep02 performance beat (docs/examples, Shot 3), verbatim.
const BEAT: &str = r#"<timeline duration="1.4">
  <track subject="camera">
    ::camera{focus="bianca" zoom="1.35" duration="0.4"}
    ::camera{shake="0.6" duration="0.3" at="0.5"}
  </track>
  <track channel="fg">
    ::cut{assetId="CUT.x.01" at="0.5"}
    ::cut{assetId="CUT.x.01" action="hide" at="1.1"}
  </track>
  <track channel="vfx">
    ::vfx{type="whiteOut" transition="flash" at="0.5"}
  </track>
  <track channel="sfx">
    ::sfx{sound="beam" assetId="P_beam" at="0.5"}
  </track>
</timeline>
:line[narrator]: After."#;

#[test]
fn clips_emit_in_at_then_track_order_with_stamps_and_barrier() {
    let (recs, _) = walk(BEAT);
    let desc: Vec<String> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Camera(c) => format!("camera@{}", c.stamp.at.unwrap()),
            Command::Cut(c) => format!("cut@{}", c.stamp.at.unwrap()),
            Command::Vfx(c) => format!("vfx@{}", c.stamp.at.unwrap()),
            Command::Sfx(c) => format!("sfx@{}", c.stamp.at.unwrap()),
            Command::Barrier(b) => format!("barrier@{}", b.at),
            Command::Line(_) => "line".to_string(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        desc,
        vec![
            "camera@0", // zoom, omitted at => track cursor 0.0
            "camera@0.5",
            "cut@0.5",
            "vfx@0.5",
            "sfx@0.5",
            "cut@1.1",
            "barrier@1.4", // authored duration wins (§11.4)
            "line",
        ]
    );
    // Every clip record is stamped with the document-order timeline ordinal.
    for r in &recs[..6] {
        let mut c = r.cmd.clone();
        let stamp = c.stamp_mut().expect("clip records are stamped").clone();
        assert_eq!(stamp.timeline, Some(1));
    }
    // Durations stamp through (zoom clip: 0.4).
    let Command::Camera(zoom) = &recs[0].cmd else { panic!() };
    assert_eq!(zoom.stamp.duration, Some(0.4));
    let Command::Barrier(b) = &recs[6].cmd else { panic!() };
    assert_eq!(b.timeline, 1);
}

#[test]
fn stage_changing_clip_threads_the_reducer_and_carries_post_barrier_state() {
    // bianca is on stage; a ::bg clip INSIDE the timeline is a scene change:
    // the auto-hide injects as a timeline-stamped record, and the walker's
    // post-barrier state carries the new bg forward.
    let body = r#"::auto{character="bianca" anchor="center" action="fade-in-up"}
<timeline>
  <track channel="scene">
    ::bg{location="street" time="night"}
  </track>
</timeline>"#;
    let (recs, state) = walk(body);
    let kinds: Vec<&str> = recs
        .iter()
        .map(|r| match &r.cmd {
            Command::Sprite(s) if s.exit == Some(true) => "hide",
            Command::Sprite(_) => "sprite",
            Command::Background(_) => "background",
            Command::Barrier(_) => "barrier",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["sprite", "hide", "background", "barrier"]);
    // The injected hide is stamped as part of the timeline too.
    let Command::Sprite(h) = &recs[1].cmd else { panic!() };
    assert_eq!(h.stamp.timeline, Some(1));
    assert_eq!(h.stamp.at, Some(0.0));
    // Post-barrier carry: stage cleared, bg recorded.
    assert!(state.on_stage.is_empty());
    assert_eq!(state.bg.as_deref(), Some("street"));
}

#[test]
fn second_timeline_gets_ordinal_two_and_barrier_defaults_to_max_end() {
    let body = r#"<timeline>
  <track channel="sfx">
    ::sfx{sound="a"}
  </track>
</timeline>
<timeline>
  <track subject="camera">
    ::camera{zoom="1.2" duration="0.7"}
  </track>
</timeline>"#;
    let (recs, _) = walk(body);
    let barriers: Vec<(u32, f64)> = recs
        .iter()
        .filter_map(|r| match &r.cmd {
            Command::Barrier(b) => Some((b.timeline, b.at)),
            _ => None,
        })
        .collect();
    // No authored duration => barrier at max clip end (0.0 and 0.7).
    assert_eq!(barriers, vec![(1, 0.0), (2, 0.7)]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test timeline`
Expected: FAIL — `clips_emit_in_at_then_track_order_with_stamps_and_barrier` panics on `unexpected Line(..)`/empty records (the Timeline arm is still a no-op, so only the trailing line emits).

- [ ] **Step 3: Write minimal implementation**

Create `crates/lute-compile/src/schedule.rs`:

```rust
//! Timeline clip scheduling (§5 pass 5): REUSES `lute-check::timeline`'s
//! cursor/barrier math (`resolve_timeline`) and zips its rows — emitted one
//! per clip, per track, in document order — back onto the clip nodes, then
//! orders deterministically by `(at, track index, clip index)`. Invoked by
//! `stage.rs` DURING the CFG walk; never a separate pass.

use lute_check::{resolve_timeline, Ctx};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{ClipNode, Timeline};

/// One clip with its resolved absolute placement.
pub struct ScheduledClip<'a> {
    pub at: f64,
    pub duration: f64,
    pub track: usize,
    pub node: &'a ClipNode,
}

/// Resolve and order a `<timeline>`'s clips; returns them with `barrier_at`
/// (authored `duration` when present, else max resolved clip end — §11.4).
/// Total: a row/clip count mismatch (impossible by construction) degrades to
/// the zipped prefix.
pub fn schedule_timeline<'a>(
    tl: &'a Timeline,
    ctx: &Ctx<'_>,
    snapshot: &CapabilitySnapshot,
) -> (Vec<ScheduledClip<'a>>, f64) {
    let (resolved, _diags) = resolve_timeline(tl, ctx, snapshot);
    let mut rows = resolved.rows.iter();
    let mut clips = Vec::new();
    for (track_ix, track) in tl.tracks.iter().enumerate() {
        for clip in &track.clips {
            let Some(row) = rows.next() else { break };
            clips.push(ScheduledClip {
                at: row.at,
                duration: row.duration,
                track: track_ix,
                node: &clip.node,
            });
        }
    }
    // Deterministic playback order: (at, track index); the sort is stable, so
    // same-(at, track) clips keep document order.
    clips.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.track.cmp(&b.track)));
    (clips, resolved.barrier_at)
}
```

In `crates/lute-compile/src/stage.rs`:

(a) extend imports:

```rust
use lute_check::Ctx;
use lute_syntax::ast::{ClipNode, Timeline};

use crate::schedule::schedule_timeline;
```

(b) replace the `Node::Timeline(_) => { … }` no-op arm in `walk_seq` with:

```rust
            Node::Timeline(tl) => {
                state = walk_timeline(em, tl, state, cx);
            }
```

(c) add `walk_timeline`:

```rust
/// §5 pass 5, inline: schedule via `lute-check::timeline` math, thread every
/// clip through the SAME reducer in `(at, track)` order, stamp
/// `timeline`/`at`(+`duration`) on every emitted record (injected ones too),
/// append the `barrier`, and carry the post-barrier state forward. Ordering
/// is load-bearing: the node AFTER the timeline injects from the timeline's
/// resulting stage, never stale pre-timeline state.
fn walk_timeline(
    em: &mut Emitter,
    tl: &Timeline,
    mut state: StageState,
    cx: &mut WalkCx<'_>,
) -> StageState {
    cx.timelines += 1;
    let ordinal = cx.timelines;
    let (clips, barrier_at) = {
        let ctx = Ctx {
            env: cx.env,
            in_match: false,
            match_subject: None,
        };
        schedule_timeline(tl, &ctx, cx.snapshot)
    };
    for sc in &clips {
        let node = match sc.node {
            ClipNode::Directive(d) => Node::Directive(d.clone()),
            ClipNode::Set(s) => Node::Set(s.clone()),
        };
        state = emit_primitive(
            em,
            &node,
            state,
            &[], // no linear lookahead inside a scheduled group
            cx,
            Some(ClipStamp {
                timeline: ordinal,
                at: sc.at,
                duration: sc.duration,
            }),
        );
    }
    em.push(Command::Barrier(BarrierCmd {
        addr: String::new(),
        timeline: ordinal,
        at: barrier_at,
    }));
    state
}
```

Add `pub mod schedule;` to `crates/lute-compile/src/lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-compile --test timeline`
Expected: PASS — 3 tests ok.
Regression: `cargo test -p lute-compile --test flatten --test inject`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/schedule.rs crates/lute-compile/src/stage.rs crates/lute-compile/src/lib.rs crates/lute-compile/tests/timeline.rs
git commit -m "feat(compile): inline timeline scheduling + barrier (pass 5)"
```

---

### Task 11: Addressing + identity (`address.rs`, P8)

Final pass (§5 pass 6): dense per-shot `addr` (`"{shot:03}-{idx:04}"`, idx += 100 over ALL emitted records — authored + injected), symbolic-label resolution (incl. trailing end-of-shot converges), `lineId` on every line + option label, `voiceKey` on voiced lines, per-speaker `code` back-fill mirroring `tag.rs` (existing codes honored; new ones step +10 above that speaker's max, format `{:04}`).

**Files:**
- Create: `crates/lute-compile/src/address.rs`
- Modify: `crates/lute-compile/src/lib.rs` (add `pub mod address;`)
- Test: `crates/lute-compile/tests/address.rs`

**Interfaces:**
- Consumes: Task 8's `cfg::{Rec, Label}` (+ `Label::parse_sym`); Task 2's `Command::{addr_mut, for_each_target}`, `LineCmd { role, speaker, line_id, voice_key, code, … }`, `ChoiceCmd { branch_id, options, … }`, `Role::voiced()`; `Diagnostic`/`Severity`/`Layer`/`Span` from `lute_core_span`.
- Produces (Task 12 consumes):

```rust
pub struct ShotRecords { pub shot: i64, pub recs: Vec<Rec>, pub trailing: Vec<Label> }
pub struct IdCx<'a> { pub character: &'a str, pub season: i64, pub episode: i64 }
pub fn assign_addresses(shots: Vec<ShotRecords>, cx: &IdCx<'_>) -> (Vec<Command>, Vec<Diagnostic>)
```

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/address.rs`:

```rust
//! Addressing + identity (§4.2): dense +100 addrs, label resolution (incl.
//! one-past-end converges), lineId/voiceKey derivation, per-speaker code
//! back-fill mirroring `lute tag`.

use lute_check::ctx::Env;
use lute_check::StageState;
use lute_compile::address::{assign_addresses, IdCx, ShotRecords};
use lute_compile::cfg::Emitter;
use lute_compile::stage::{walk_seq, WalkCx};
use lute_compile::Command;
use lute_core_span::Severity;

/// Walk every shot of `src` and address the result — the same wiring
/// `compile()` (Task 12) uses.
fn addressed(src: &str) -> (Vec<Command>, Vec<lute_core_span::Diagnostic>) {
    let (doc, diags) = lute_syntax::parse(src);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let env = Env::default();
    let mut cx = WalkCx {
        snapshot: &snapshot,
        env: &env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut state = StageState::default();
    let mut shots = Vec::new();
    let mut prev = 0i64;
    for (i, shot) in doc.shots.iter().enumerate() {
        let mut em = Emitter::default();
        state = walk_seq(&mut em, &shot.body, state, &mut cx);
        let authored = shot.number.unwrap_or(i as i64 + 1);
        let shot_no = authored.max(prev + 1);
        prev = shot_no;
        let (recs, trailing) = em.finish();
        shots.push(ShotRecords { shot: shot_no, recs, trailing });
    }
    assign_addresses(shots, &IdCx { character: "bianca", season: 1, episode: 2 })
}

const SRC: &str = r#"---
character: bianca
season: 1
episode: 2
---

## Shot 1.

:line[fixer]{code="0010"}: ...
:line[narrator]: He waited.

## Shot 4.

:line[fixer]{code="0050"}: Bianca. Your number.
:line[fixer]: And again.
<branch id="number">
  <choice id="blunt" label="Just ask, flatly">
    :line[bianca]{code="0010" emotion="surprised"}: Oh!
  </choice>
  <choice id="soft" label="Ask gently">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>
"#;

#[test]
fn addrs_are_dense_per_shot_and_labels_resolve() {
    let (cmds, diags) = addressed(SRC);
    assert!(diags.is_empty(), "{diags:#?}");
    let addrs: Vec<String> = cmds
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.addr_mut().clone()
        })
        .collect();
    // Shot 1: two lines. Shot 4 (authored number): line, line, choice header,
    // arm records + jumps.
    assert_eq!(addrs[0], "001-0100");
    assert_eq!(addrs[1], "001-0200");
    assert_eq!(addrs[2], "004-0100");
    // +100 gaps, strictly increasing within each shot.
    for w in addrs.windows(2) {
        assert!(w[0] < w[1], "addr order: {w:?}");
    }
    // Every control-flow target resolved to a real addr — no symbolic '@'.
    for c in &cmds {
        let mut c = c.clone();
        c.for_each_target(&mut |t: &mut String| {
            assert!(!t.starts_with('@'), "unresolved label {t}");
            assert_eq!(t.len(), 8, "addr shape: {t}");
        });
    }
    // The branch is the LAST node of shot 4: its converge resolved to the
    // one-past-end addr (plan spec-gap note 2).
    let Some(Command::Choice(choice)) = cmds
        .iter()
        .find(|c| matches!(c, Command::Choice(_)))
    else {
        panic!()
    };
    let last_addr = {
        let mut last = cmds.last().unwrap().clone();
        last.addr_mut().clone()
    };
    let expected_past_end = format!(
        "004-{:04}",
        last_addr[4..].parse::<i64>().unwrap() + 100
    );
    assert_eq!(choice.converge, expected_past_end);
    // Option targets point at the arms' first records.
    let Command::Line(blunt_line) = &cmds[5] else { panic!("{:#?}", cmds) };
    assert_eq!(choice.options[0].target, "004-0400");
    assert_eq!(blunt_line.text, "Oh!");
}

#[test]
fn line_ids_and_voice_keys_follow_the_speaker_code_model() {
    let (cmds, _) = addressed(SRC);
    let lines: Vec<(&str, &str, Option<&str>)> = cmds
        .iter()
        .filter_map(|c| match c {
            Command::Line(l) => Some((
                l.speaker.as_str(),
                l.line_id.as_str(),
                l.voice_key.as_deref(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            // Authored code kept.
            ("fixer", "bianca.s01ep02.fixer_0010", Some("fixer-0010")),
            // Narrator: lineId for i18n, NO voiceKey (unvoiced role).
            ("narrator", "bianca.s01ep02.narrator_0010", None),
            ("fixer", "bianca.s01ep02.fixer_0050", Some("fixer-0050")),
            // Back-filled: fixer's max authored code is 0050 => next is 0060.
            ("fixer", "bianca.s01ep02.fixer_0060", Some("fixer-0060")),
            ("bianca", "bianca.s01ep02.bianca_0010", Some("bianca-0010")),
        ]
    );
    // Option labels get structural lineIds: {character}.s{s}ep{e}.{branchId}.{choiceId}.
    let Some(Command::Choice(choice)) = cmds
        .iter()
        .find(|c| matches!(c, Command::Choice(_)))
    else {
        panic!()
    };
    assert_eq!(choice.options[0].line_id, "bianca.s01ep02.number.blunt");
    assert_eq!(choice.options[1].line_id, "bianca.s01ep02.number.soft");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test address`
Expected: FAIL — `error[E0432]: unresolved import lute_compile::address`.

- [ ] **Step 3: Write minimal implementation**

Create `crates/lute-compile/src/address.rs`:

```rust
//! §5 pass 6 — addressing + identity. `addr` is regenerated each compile (a
//! position); `lineId`/`voiceKey` are stable content joins derived from the
//! per-speaker `code` (dsl §12's Yarn `#line:` model — `lute tag` persists
//! codes into source; this pass only back-fills the not-yet-tagged remainder
//! deterministically, never rewriting source).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};

use crate::cfg::{Label, Rec};
use crate::ir::Command;

/// One shot's emitted records + labels left trailing past its end.
pub struct ShotRecords {
    pub shot: i64,
    pub recs: Vec<Rec>,
    pub trailing: Vec<Label>,
}

/// Identity context for `lineId`/`voiceKey` derivation (§4.2).
pub struct IdCx<'a> {
    pub character: &'a str,
    pub season: i64,
    pub episode: i64,
}

/// Assign every `addr`, resolve every symbolic target, and stamp identity.
/// Returns the flat command array in final order. An unresolved label is a
/// compiler bug surfaced as `E-COMPILE-INTERNAL` (never a panic, D6 aborts).
pub fn assign_addresses(shots: Vec<ShotRecords>, cx: &IdCx<'_>) -> (Vec<Command>, Vec<Diagnostic>) {
    let mut out: Vec<Command> = Vec::new();
    let mut diags: Vec<Diagnostic> = Vec::new();
    for shot in shots {
        // Label -> concrete addr (labels are per-shot, so the map is too).
        let mut labels: BTreeMap<u32, String> = BTreeMap::new();
        for (i, rec) in shot.recs.iter().enumerate() {
            let addr = addr_of(shot.shot, i);
            for l in &rec.labels {
                labels.insert(l.0, addr.clone());
            }
        }
        // End-of-shot converge: one past the last record (spec-gap note 2).
        let past_end = addr_of(shot.shot, shot.recs.len());
        for l in &shot.trailing {
            labels.insert(l.0, past_end.clone());
        }
        for (i, mut rec) in shot.recs.into_iter().enumerate() {
            *rec.cmd.addr_mut() = addr_of(shot.shot, i);
            rec.cmd.for_each_target(&mut |t: &mut String| {
                if let Some(n) = Label::parse_sym(t) {
                    match labels.get(&n) {
                        Some(addr) => *t = addr.clone(),
                        None => diags.push(internal(format!(
                            "unresolved control-flow label `@{n}` in shot {}",
                            shot.shot
                        ))),
                    }
                }
            });
            out.push(rec.cmd);
        }
    }
    assign_identity(&mut out, cx);
    (out, diags)
}

/// `"{shot:03}-{idx:04}"` with idx = (position+1) * 100 — the +100 gaps leave
/// room to hand-insert a row (§4.2).
fn addr_of(shot: i64, position: usize) -> String {
    format!("{:03}-{:04}", shot, (position as i64 + 1) * 100)
}

/// `lineId` on every line + option label; `voiceKey` on voiced lines; codes
/// back-filled per speaker (max authored + 10 steps, `{:04}` — tag.rs's
/// scheme).
fn assign_identity(cmds: &mut [Command], cx: &IdCx<'_>) {
    // Pass 1: per-speaker highest AUTHORED numeric code.
    let mut max_code: BTreeMap<String, u64> = BTreeMap::new();
    for cmd in cmds.iter() {
        if let Command::Line(l) = cmd {
            if let Some(n) = l.code.as_deref().and_then(|c| c.parse::<u64>().ok()) {
                let e = max_code.entry(l.speaker.clone()).or_insert(0);
                if n > *e {
                    *e = n;
                }
            }
        }
    }
    // Pass 2, final record order: fill codes, derive ids.
    let prefix = format!("{}.s{:02}ep{:02}", cx.character, cx.season, cx.episode);
    for cmd in cmds.iter_mut() {
        match cmd {
            Command::Line(l) => {
                let code = match &l.code {
                    Some(c) => c.clone(),
                    None => {
                        let e = max_code.entry(l.speaker.clone()).or_insert(0);
                        *e += 10;
                        format!("{:04}", *e)
                    }
                };
                l.line_id = format!("{prefix}.{}_{}", l.speaker, code);
                if l.role.voiced() {
                    // v1: voiceKey bank == characterId == the speaker (§11).
                    l.voice_key = Some(format!("{}-{}", l.speaker, code));
                }
                l.code = Some(code);
            }
            Command::Choice(c) => {
                for o in &mut c.options {
                    o.line_id = format!("{prefix}.{}.{}", c.branch_id, o.id);
                }
            }
            _ => {}
        }
    }
}

fn internal(message: String) -> Diagnostic {
    Diagnostic {
        code: "E-COMPILE-INTERNAL".to_string(),
        severity: Severity::Error,
        message,
        span: Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
    }
}
```

Add `pub mod address;` to `crates/lute-compile/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile --test address`
Expected: PASS — 2 tests ok. (Shot-4 dense order: line `004-0100`, line `004-0200`, choice `004-0300`, blunt-line `004-0400`, jump `004-0500`, set `004-0600`, jump `004-0700`; the trailing converge resolves to `004-0800`, one past the last record — so `cmds[5]` is the blunt line after the two shot-1 records.)

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/address.rs crates/lute-compile/src/lib.rs crates/lute-compile/tests/address.rs
git commit -m "feat(compile): addressing + lineId/voiceKey identity (pass 6)"
```

---

### Task 12: `compile()` orchestration + artifact envelope (`lib.rs`, P9a)

Wire the whole §5 pipeline: gate on a clean `check()` (D6) → re-parse + fill → `fold_env` → normalize (D8) → expand (D4) → per-shot stage walk → addressing → envelope (folded `state`, meta with `episodeId`/`title`).

**Files:**
- Modify: `crates/lute-compile/src/lib.rs`
- Test: `crates/lute-compile/tests/compile.rs`

**Interfaces:**
- Consumes: `lute_check::{check, fold_env, CheckInput, CheckResult, FoldedEnv, StageState}` (Task 4); `lute_cel::{fill_document, CelArena}` (`fill_document(arena: &mut CelArena, doc: &mut Document) -> Vec<CelParseError>`); `lute_syntax::parse(text: &str) -> (Document, Vec<Diagnostic>)`; Tasks 5–11 modules; `StateSchema { pub decls: BTreeMap<String, StateDecl> }`, `StateDecl { pub ty: Type, pub default: Option<Literal>, … }`, `Type::{Bool, Number, Str, Enum, List, Record, Map, EnumFromOption, ProviderRef, SlotId, AssetKind}`, `Literal::{Bool, Num, Str, List, Map}`.
- Produces (Task 13/14 consume): `pub fn compile(input: &CheckInput) -> Result<Artifact, Vec<Diagnostic>>`.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/compile.rs`:

```rust
//! `compile()` orchestration: the D6 gate, the folded-state envelope, id
//! stamping, CEL expansion in situ, and byte determinism.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, Command};

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    }
}

const SCENE: &str = r#"---
character: bianca
season: 1
episode: 2
title: Compile me
state:
  scene.affect.bianca: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.bianca >= 1" }
---

## Shot 1.

::bg{location="family_restaurant" time="afternoon" assetId="BG.x"}
::auto{character="bianca" action="fade-in-up"}
:line[bianca]{code="0010" emotion="surprised"}: Oh!

<branch id="number">
  <choice id="blunt" label="Flat">
    :line[fixer]{code="0010"}: Number.
  </choice>
  <choice id="soft" label="Gentle">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>

<match on="scene.choices.number">
  <when test="@fond">
    :line[fixer]{delivery="thought"}: Nice.
  </when>
  <when test="$ == 'blunt'">
    :line[fixer]{delivery="thought"}: Flat.
  </when>
  <otherwise>
    :line[fixer]{delivery="thought"}: Hm.
  </otherwise>
</match>
"#;

#[test]
fn error_doc_emits_no_artifact() {
    // Undeclared state write => Error diagnostic => gate refuses (D6).
    let bad = "---\ncharacter: b\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n::set{scene.nope = 1}\n";
    let err = compile(&input(bad)).unwrap_err();
    assert!(
        err.iter().any(|d| d.code == "E-UNDECLARED"),
        "{err:#?}"
    );
}

#[test]
fn clean_doc_compiles_with_envelope_expansion_and_ids() {
    let artifact = compile(&input(SCENE)).expect("clean compile");
    assert_eq!(artifact.lute, "0.0.1");
    assert_eq!(artifact.meta.character, "bianca");
    assert_eq!(artifact.meta.episode_id, "S01EP02");
    assert_eq!(artifact.meta.title.as_deref(), Some("Compile me"));

    // Folded state envelope: author decl + implicit branch decl (§4.1).
    let paths: Vec<&str> = artifact.state.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, vec!["scene.affect.bianca", "scene.choices.number"]);
    let choice_entry = &artifact.state[1];
    assert_eq!(choice_entry.ty, "enum");
    assert_eq!(
        choice_entry.domain.as_deref(),
        Some(["blunt".to_string(), "soft".to_string(), "unset".to_string()].as_slice())
    );
    assert_eq!(choice_entry.provenance.as_deref(), Some("branch:number"));
    let affect = &artifact.state[0];
    assert_eq!(affect.ty, "number");
    assert_eq!(affect.default, Some(serde_json::json!(0)));

    // First record: the bg, addressed densely.
    let json = serde_json::to_value(&artifact.commands[0]).unwrap();
    assert_eq!(json["kind"], "background");
    assert_eq!(json["addr"], "001-0100");

    // Match arms expanded: @fond parenthesized; $ replaced by the subject.
    let m = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Match(m) => Some(m),
            _ => None,
        })
        .expect("match record");
    assert_eq!(m.arms[0].test, "(scene.affect.bianca >= 1)");
    assert_eq!(m.arms[1].test, "scene.choices.number == 'blunt'");
    assert!(m.otherwise.is_some());

    // No symbolic labels or DSL tokens survive anywhere.
    let all = serde_json::to_string(&artifact).unwrap();
    assert!(!all.contains("\"@"), "unexpanded/unresolved: {all}");
    assert!(!all.contains("textUnitId"));

    // Back-filled thought-line ids (fixer max authored 0010 -> 0020/0030/0040),
    // monologue => no voiceKey.
    let thoughts: Vec<(&str, Option<&str>)> = artifact
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::Line(l) if l.text != "Number." && l.speaker == "fixer" => {
                Some((l.line_id.as_str(), l.voice_key.as_deref()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        thoughts,
        vec![
            ("bianca.s01ep02.fixer_0020", None),
            ("bianca.s01ep02.fixer_0030", None),
            ("bianca.s01ep02.fixer_0040", None),
        ]
    );
}

#[test]
fn injection_warnings_do_not_gate_and_output_is_byte_stable() {
    // The ::auto has no anchor => an anchor is INJECTED (a warning-free case);
    // W-INJECT-CONFLICT-class warnings never gate (only Errors do, D6).
    let a1 = compile(&input(SCENE)).expect("ok");
    let a2 = compile(&input(SCENE)).expect("ok");
    let s1 = serde_json::to_string_pretty(&a1).unwrap();
    let s2 = serde_json::to_string_pretty(&a2).unwrap();
    assert_eq!(s1, s2, "same input => byte-identical artifact");
    // And serializing the SAME artifact twice is stable too.
    assert_eq!(s1, serde_json::to_string_pretty(&a1).unwrap());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test compile`
Expected: FAIL — `error[E0432]: unresolved import lute_compile::compile`.

- [ ] **Step 3: Write minimal implementation**

Replace `crates/lute-compile/src/lib.rs` with (keeps every existing module decl; adds orchestration):

```rust
//! `lute-compile` — lowers a checked `.lute` document to the typed JSON
//! command-record artifact (design spec
//! `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`).
//!
//! Pipeline (§5): check gate (D6) -> normalize (D8) -> expand (D4) ->
//! flatten + CFG-aware stage resolution incl. inline timelines (D9) ->
//! addressing + identity -> deterministic serialization.

pub mod address;
pub mod cfg;
pub mod expand;
pub mod ir;
pub mod lower;
pub mod normalize;
pub mod schedule;
pub mod stage;

pub use ir::*;

use lute_cel::CelArena;
use lute_check::meta::StateSchema;
use lute_check::{check, fold_env, CheckInput, FoldedEnv, StageState};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::Document;

/// IR version stamped into every artifact envelope (`"lute": …`, spec §4.1).
pub const LUTE_IR_VERSION: &str = "0.0.1";

/// Compile a checked document to its artifact. `Err` carries the gating
/// diagnostics: the full `check()` stream when any Error is present (D6), or
/// compile-stage errors (`E-COMPILE-*`). Never panics.
pub fn compile(input: &CheckInput) -> Result<Artifact, Vec<Diagnostic>> {
    // D6 gate: codegen runs only on a clean check, so every pass below may
    // RELY on checker-proven invariants (declared paths, exhaustiveness,
    // acyclic components, @ref arity, unique choice ids via E-CHOICE-DUP).
    let result = check(input);
    if !result.ok {
        return Err(result.diagnostics);
    }

    // Re-derive the parsed, CEL-filled document + the folded environment
    // (fold diagnostics were already reported by the gate run).
    let (mut doc, _) = lute_syntax::parse(&input.text);
    let mut arena = CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _) = fold_env(&doc, input);

    // §5 pass 2 — AST normalization (D8): components + persist.
    let mut diags = normalize::normalize_document(&mut doc, &input.components, &folded.env.state);

    // §5 pass 3 — CEL expansion (D4).
    let table = expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    diags.extend(expand::expand_document(&mut doc, &table));

    // §5 passes 4–5 — flatten + CFG-aware stage resolution + inline timelines.
    let mut cx = stage::WalkCx {
        snapshot: &input.snapshot,
        env: &folded.env,
        components: Vec::new(),
        timelines: 0,
    };
    let mut state = StageState::default();
    let mut shots = Vec::new();
    let mut prev_shot = 0i64;
    for (i, shot) in doc.shots.iter().enumerate() {
        let mut em = cfg::Emitter::default();
        state = stage::walk_seq(&mut em, &shot.body, state, &mut cx);
        // Authored shot number when present; strictly increasing guard keeps
        // addrs unique if headings repeat or regress.
        let authored = shot.number.unwrap_or(i as i64 + 1);
        let shot_no = authored.max(prev_shot + 1);
        prev_shot = shot_no;
        let (recs, trailing) = em.finish();
        shots.push(address::ShotRecords {
            shot: shot_no,
            recs,
            trailing,
        });
    }
    // Our fold re-derives W-INJECT-CONFLICTs check() already reported —
    // check() is the diagnostic surface, the artifact is ours (plan note 8).
    state.diags.clear();

    // §5 pass 6 — addressing + identity.
    let meta = artifact_meta(&doc, &folded);
    let idcx = address::IdCx {
        character: &meta.character,
        season: meta.season,
        episode: meta.episode,
    };
    let (commands, addr_diags) = address::assign_addresses(shots, &idcx);
    diags.extend(addr_diags);

    if diags.iter().any(|d| d.severity == Severity::Error) {
        return Err(diags);
    }
    Ok(Artifact {
        lute: LUTE_IR_VERSION.to_string(),
        meta,
        state: state_entries(&folded.env.state),
        commands,
    })
}

/// Envelope meta (§4.1). `character`/`season`/`episode` are §6.1 REQUIRED
/// keys — the gate proved them present; degrade to defaults, never panic.
/// `title` is read from the raw frontmatter (plan spec-gap note 3).
fn artifact_meta(doc: &Document, folded: &FoldedEnv) -> ArtifactMeta {
    let character = folded.typed.character.clone().unwrap_or_default();
    let season = folded.typed.season.unwrap_or(0);
    let episode = folded.typed.episode.unwrap_or(0);
    let title = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml)
        .ok()
        .and_then(|m| {
            m.get(serde_yaml::Value::String("title".to_string()))
                .and_then(|v| v.as_str().map(String::from))
        });
    ArtifactMeta {
        character,
        season,
        episode,
        episode_id: format!("S{:02}EP{:02}", season, episode),
        title,
    }
}

/// The RESOLVED + FOLDED state table (§4.1): BTreeMap order = sorted by path
/// (deterministic). Implicit `scene.choices.*` entries append `unset` to
/// their domain and carry `branch:<id>` provenance (§11.1, plan note 10).
fn state_entries(schema: &StateSchema) -> Vec<StateEntry> {
    schema
        .decls
        .iter()
        .map(|(path, decl)| {
            let (ty, domain) = type_label(path, &decl.ty);
            StateEntry {
                path: path.clone(),
                ty,
                domain,
                default: decl.default.as_ref().map(literal_json),
                provenance: path
                    .strip_prefix("scene.choices.")
                    .map(|id| format!("branch:{id}")),
            }
        })
        .collect()
}

fn type_label(path: &str, ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => {
            let mut domain = members.clone();
            if path.starts_with("scene.choices.") {
                domain.push("unset".to_string());
            }
            ("enum".to_string(), Some(domain))
        }
        Type::List(_) => ("list".to_string(), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { .. } => ("map".to_string(), None),
        // Id-flavored types are strings at the value level (§7 plugin types).
        Type::EnumFromOption(_) => ("enum".to_string(), None),
        Type::ProviderRef(_) | Type::SlotId { .. } | Type::AssetKind(_) => {
            ("string".to_string(), None)
        }
    }
}

/// Manifest literal -> JSON. Integral floats collapse to JSON integers so the
/// envelope reads `0`, not `0.0` (§4.1 example).
fn literal_json(l: &Literal) -> serde_json::Value {
    match l {
        Literal::Bool(b) => serde_json::Value::Bool(*b),
        Literal::Num(n) if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.0e15 => {
            serde_json::Value::from(*n as i64)
        }
        Literal::Num(n) => serde_json::Value::from(*n),
        Literal::Str(s) => serde_json::Value::String(s.clone()),
        Literal::List(xs) => serde_json::Value::Array(xs.iter().map(literal_json).collect()),
        Literal::Map(m) => serde_json::Value::Object(
            m.iter().map(|(k, v)| (k.clone(), literal_json(v))).collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ir_version_matches_language_version() {
        assert_eq!(super::LUTE_IR_VERSION, "0.0.1");
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile --test compile`
Expected: PASS — 3 tests ok.
Full-crate regression: `cargo test -p lute-compile`
Expected: PASS — all suites (ir_golden, flatten, inject, timeline, address, compile, unit tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/src/lib.rs crates/lute-compile/tests/compile.rs
git commit -m "feat(compile): compile() orchestration + artifact envelope (D6 gate)"
```

---

### Task 13: `lute compile` CLI subcommand (P9b)

Thin shell (D7): read → `lute_compile::compile()` → write pretty JSON to `-o`/stdout. Exit `0` clean / `1` check-error / `2` I/O. The capability-surface resolution (project snapshot, providers, `uses:`/`components:` imports) is EXACTLY `run_check`'s — extracted into a shared `build_input` so the two subcommands can never diverge.

**Files:**
- Modify: `crates/lute-cli/Cargo.toml` (add the `lute-compile` dependency)
- Modify: `crates/lute-cli/src/main.rs` (Command enum ~line 41; `run_check` ~line 100: extract `build_input`; add `run_compile`)
- Test: `crates/lute-cli/tests/compile.rs`

**Interfaces:**
- Consumes: Task 12's `lute_compile::compile(&CheckInput) -> Result<Artifact, Vec<Diagnostic>>`; the existing `main.rs` helpers `severity_str(Severity) -> &'static str` and the resolution calls already inside `run_check` (`load_project`, `ProviderSet::load`, `project_providers`, `parse_meta`, `resolve_document_snapshot`, `resolve_imports`, `resolve_components`).
- Produces: `lute compile <file> [--json] [--providers <DIR>] [--project <DIR>] [-o <FILE>]`.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-cli/tests/compile.rs`:

```rust
//! `lute compile` acceptance: exit codes 0/1/2, stdout artifact JSON, `-o`.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

#[test]
fn compile_bianca_exits_zero_with_artifact_json() {
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/bianca-s01ep02.lute"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lute"], "0.0.1");
    assert_eq!(v["meta"]["episodeId"], "S01EP02");
    let commands = v["commands"].as_array().unwrap();
    assert!(!commands.is_empty());
    assert_eq!(commands[0]["addr"], "001-0100");
}

#[test]
fn compile_error_doc_exits_one_and_emits_no_artifact() {
    // date-minigame needs its project; core-only it checks with errors.
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/date-minigame.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty() || !out.stdout.starts_with(b"{"), "no artifact on stdout");
}

#[test]
fn compile_missing_file_exits_two() {
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/no-such-file.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn compile_writes_out_file() {
    let tmp = std::env::temp_dir().join("lute-compile-cli-test.json");
    let _ = std::fs::remove_file(&tmp);
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/bianca-s01ep02.lute", "-o"])
        .arg(&tmp)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "artifact goes to the file, not stdout");
    let s = std::fs::read_to_string(&tmp).unwrap();
    assert!(s.starts_with("{\n"));
    assert!(s.ends_with("\n"));
    let _ = std::fs::remove_file(&tmp);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-cli --test compile`
Expected: FAIL — exit code mismatch / clap error `unrecognized subcommand 'compile'` (surfaces as code 2 from clap; the bianca test asserts 0 and fails).

- [ ] **Step 3: Write minimal implementation**

(a) `crates/lute-cli/Cargo.toml` — add under `[dependencies]`:

```toml
lute-compile = { path = "../lute-compile" }
```

(b) `crates/lute-cli/src/main.rs` — add the variant to `enum Command` (after `Check { .. }`):

```rust
    /// Compile a checked `.lute` document to its JSON command-record artifact.
    Compile {
        /// Path to the `.lute` file to compile.
        file: PathBuf,
        /// On a failed gate, print the diagnostics as JSON instead of
        /// human-readable lines. (The artifact itself is always JSON.)
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) resolving the
        /// document's activated capability snapshot.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Write the artifact here instead of stdout.
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
    },
```

and the dispatch arm in `main()`:

```rust
        Command::Compile {
            file,
            json,
            providers,
            project,
            out,
        } => run_compile(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            out.as_deref(),
        ),
```

(c) extract the input assembly from `run_check` — everything between the successful `read_to_string` and `let result = check(&input);` moves VERBATIM into:

```rust
/// Assemble the `CheckInput` for `file` exactly as `check` does: project
/// snapshot resolution (plugin §4/§11), provider-catalog precedence (plugin
/// §10), and `uses:`/`components:` imports resolved against the file's own
/// directory. `None` => the file could not be read (caller exits 2).
fn build_input(file: &Path, providers: Option<&Path>, project: Option<&Path>) -> Option<CheckInput> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return None;
        }
    };
    // >>> MOVED verbatim from run_check: the `project` load/fallback match,
    //     the `providers` precedence match, the `parse`/`parse_meta` lift,
    //     `resolve_document_snapshot` + its rdiags eprintln loop, and the
    //     `resolve_imports`/`resolve_components` calls against `base`.
    Some(CheckInput {
        text,
        uri: file.display().to_string(),
        snapshot,
        providers,
        mode: Mode::Ci,
        imports,
        components,
    })
}
```

`run_check` shrinks to:

```rust
fn run_check(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    let result = check(&input);
    // (unchanged: --json serialization / print_human / exit-code tail)
```

(d) add `run_compile`:

```rust
/// Run `compile` over one file. Exit `0` with the artifact on stdout (or
/// `-o <FILE>`), `1` when the check gate fails (diagnostics to stdout,
/// human or `--json`), `2` on I/O or serialization failure.
fn run_compile(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    out: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    match lute_compile::compile(&input) {
        Ok(artifact) => {
            let mut s = match serde_json::to_string_pretty(&artifact) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("lute: failed to serialize artifact: {e}");
                    return ExitCode::from(2);
                }
            };
            s.push('\n');
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, &s) {
                        eprintln!("lute: cannot write {}: {e}", path.display());
                        return ExitCode::from(2);
                    }
                }
                None => print!("{s}"),
            }
            ExitCode::SUCCESS
        }
        Err(diags) => {
            if json {
                match serde_json::to_string_pretty(&diags) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                for d in &diags {
                    println!(
                        "{}:{}:{}: {} [{}] {}",
                        file.display(),
                        d.span.line,
                        d.span.column,
                        severity_str(d.severity),
                        d.code,
                        d.message
                    );
                }
                let errors = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .count();
                println!("{errors} error(s); no artifact emitted");
            }
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-cli --test compile`
Expected: PASS — 4 tests ok.
Regression (the `run_check` extraction must not change behavior): `cargo test -p lute-cli`
Expected: PASS — `cli.rs`, `plugin_loaded.rs`, `asset_project.rs`, `uses_import.rs` all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-cli && cargo clippy -p lute-cli --all-targets -- -D warnings
git add crates/lute-cli/Cargo.toml crates/lute-cli/src/main.rs crates/lute-cli/tests/compile.rs
git commit -m "feat(cli): lute compile subcommand (exit 0/1/2, -o)"
```

---

### Task 14: E2E artifact goldens + determinism (P10)

Full-artifact `insta` snapshots for the three real fixtures (§8): `bianca-s01ep02.lute` (core), `showcase/episode01.lute` (project + plugins + imports + components), `components/scene.lute` (component expansion) — plus a structural invariant checker and the byte-determinism assertion.

**Files:**
- Test: `crates/lute-compile/tests/e2e.rs`
- Create (generated, then accepted): `crates/lute-compile/tests/snapshots/e2e__bianca_s01ep02.snap`, `…__showcase_episode01.snap`, `…__components_scene.snap`

**Interfaces:**
- Consumes: `lute_compile::compile`; the CLI's resolution recipe re-assembled in-test: `lute_manifest::project::{load_project, project_providers, resolve_document_snapshot}`, `lute_check::{parse_meta, resolve_imports, resolve_components}`; fixtures at `../../docs/examples/` (crate-relative).
- Produces: the executable record of "what `compile()` emits for a correct document of this shape" (mirrors `lute-check/tests/golden.rs`'s role).

- [ ] **Step 1: Write the failing test**

Create `crates/lute-compile/tests/e2e.rs`:

```rust
//! E2E artifact goldens (§8): real fixtures -> full-artifact snapshots +
//! structural invariants + byte determinism.

use std::collections::BTreeSet;
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_compile::compile;

/// Assemble the CheckInput exactly as `lute compile` (Task 13) does.
fn input_for(path: &str, project_dir: Option<&str>) -> CheckInput {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap();
    let project = project_dir.and_then(|d| {
        lute_manifest::project::load_project(Path::new(d)).expect("project loads")
    });
    let providers = lute_manifest::project::project_providers(project.as_ref());
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        project.as_ref(),
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text,
        uri: path.to_string(),
        snapshot,
        providers,
        mode: Mode::Ci,
        imports,
        components,
    }
}

/// Structural invariants every artifact must satisfy (§4, §7): unique ordered
/// addrs, fully resolved targets, +100 gapping, id discipline, no DSL tokens.
fn assert_artifact_invariants(json: &serde_json::Value) {
    let commands = json["commands"].as_array().expect("commands array");
    let mut addrs: Vec<&str> = Vec::new();
    for c in commands {
        addrs.push(c["addr"].as_str().expect("every record has addr"));
    }
    let unique: BTreeSet<&str> = addrs.iter().copied().collect();
    assert_eq!(unique.len(), addrs.len(), "addrs unique");
    let mut sorted = addrs.clone();
    sorted.sort();
    assert_eq!(sorted, addrs, "addrs strictly ascending");
    let addr_set: BTreeSet<&str> = unique;
    for c in commands {
        for key in ["target", "converge", "otherwise"] {
            if let Some(t) = c[key].as_str() {
                assert_target(t, &addr_set);
            }
        }
        for arm in c["arms"].as_array().into_iter().flatten() {
            assert_target(arm["target"].as_str().unwrap(), &addr_set);
        }
        for opt in c["options"].as_array().into_iter().flatten() {
            assert_target(opt["target"].as_str().unwrap(), &addr_set);
            assert!(opt["lineId"].as_str().is_some_and(|s| !s.is_empty()));
        }
        if c["kind"] == "line" {
            assert!(c["lineId"].as_str().is_some_and(|s| !s.is_empty()));
            let voiced = c["role"] == "dialogue" || c["role"] == "voiceover";
            assert_eq!(c["voiceKey"].is_string(), voiced, "voiceKey iff voiced: {c}");
            assert!(c.get("code").is_none(), "no standalone code field (§4.2)");
        }
    }
    // Retired identifier must not exist anywhere (§4.2).
    assert!(!json.to_string().contains("textUnitId"));
}

fn assert_target(t: &str, addrs: &BTreeSet<&str>) {
    assert!(!t.starts_with('@'), "unresolved symbolic target {t}");
    // A target is a real record OR the one-past-end converge of its shot.
    assert!(
        addrs.contains(t) || t.len() == 8,
        "malformed target {t}"
    );
}

fn golden(name: &str, path: &str, project: Option<&str>) {
    let input = input_for(path, project);
    let artifact = compile(&input).unwrap_or_else(|e| panic!("{path} compiles: {e:#?}"));
    let mut json = serde_json::to_string_pretty(&artifact).unwrap();
    json.push('\n');
    assert_artifact_invariants(&serde_json::from_str(&json).unwrap());
    // Determinism (§8): same input => byte-identical artifact.
    let again = compile(&input).expect("recompiles");
    let mut json2 = serde_json::to_string_pretty(&again).unwrap();
    json2.push('\n');
    assert_eq!(json, json2, "byte-stable across compiles");
    insta::assert_snapshot!(name, json);
}

#[test]
fn bianca_s01ep02() {
    golden("bianca_s01ep02", "../../docs/examples/bianca-s01ep02.lute", None);
}

#[test]
fn showcase_episode01() {
    golden(
        "showcase_episode01",
        "../../docs/examples/showcase/episode01.lute",
        Some("../../docs/examples/showcase"),
    );
}

#[test]
fn components_scene() {
    golden(
        "components_scene",
        "../../docs/examples/components/scene.lute",
        None,
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-compile --test e2e`
Expected: FAIL — 3 tests fail with `snapshot assertion … to review, run cargo insta review` (insta writes `.snap.new` files on first run). The invariant + determinism assertions must ALREADY PASS at this point — if one of those panics instead, that is a real pipeline bug: fix it before accepting anything.

- [ ] **Step 3: Review + accept the snapshots (this task's "implementation")**

Eyeball each generated `crates/lute-compile/tests/snapshots/e2e__*.snap.new` against the spec before accepting:
- `bianca_s01ep02`: shot-2 `::auto` sprite followed by an injected `preload` sprite with `entry-emotion-lookahead` provenance (§4.5); the shot-3 timeline records stamped `"timeline": 1` with a `barrier` at `1.4`; the shot-4 `choice` with `recordKey: "scene.choices.number"` and resolved targets; shot-5 `match` with the `@fond` arm expanded to `(scene.affect.bianca >= 1)`.
- `showcase_episode01`: plugin directives as `kind: "plugin"` passthrough records; imported (`uses:`) state in the envelope; the `::use`d stinger records carrying `source: { component: "stinger" }`.
- `components_scene`: the greet body inline with `character: "bianca"` bound and `source: { component: "greet" }`.

Then: `cargo insta accept`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-compile --test e2e`
Expected: PASS — 3 tests ok, no `.snap.new` files remaining (`ls crates/lute-compile/tests/snapshots/` shows exactly three `e2e__*.snap`).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p lute-compile && cargo clippy -p lute-compile --all-targets -- -D warnings && cargo test -p lute-compile
git add crates/lute-compile/tests/e2e.rs crates/lute-compile/tests/snapshots/
git commit -m "test(compile): e2e artifact goldens + determinism (bianca/showcase/components)"
```

---

### Task 15: Full-workspace verification gate

The end-of-plan gate (spec §10): everything green across the workspace, the tree-sitter stamp untouched, and the branch clean.

**Files:**
- None (verification only; commit only if a fix was required).

**Interfaces:**
- Consumes: everything above.
- Produces: a releasable `feat/lute-compile` branch.

- [ ] **Step 1: Format check across the workspace**

Run: `cargo fmt --all -- --check`
Expected: no output, exit 0. (If it rewrites anything, a prior task skipped its fmt step — run `cargo fmt --all`, re-run the crate's tests, and fold the fix into a `chore: fmt` commit.)

- [ ] **Step 2: Lint across the workspace**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: exit 0, zero warnings.

- [ ] **Step 3: Test across the workspace**

Run: `cargo test --workspace`
Expected: exit 0 — every suite green, including `lute-lsp`'s `divergence.rs` (the Task-4 refactor guard) and every pre-existing insta golden (no `.snap.new` anywhere: `find crates -name '*.snap.new'` prints nothing).

- [ ] **Step 4: Tree-sitter capability stamp unchanged**

Run: `cargo test -p lute-manifest --test tree_sitter_stamp`
Expected: PASS — this plan never touches `crates/lute-manifest/assets/**`, so the core `capabilityVersion` is byte-identical (spec §10: no tree-sitter impact).

- [ ] **Step 5: Determinism spot-check through the CLI**

Run:
```bash
target_bin=$(cargo build -p lute-cli 2>/dev/null; echo /Users/journey/Workspace/lute/target/debug/lute)
$target_bin compile docs/examples/bianca-s01ep02.lute -o /tmp/lute-a.json
$target_bin compile docs/examples/bianca-s01ep02.lute -o /tmp/lute-b.json
cmp /tmp/lute-a.json /tmp/lute-b.json && echo BYTE-IDENTICAL
```
Expected: `BYTE-IDENTICAL`.

- [ ] **Step 6: Working tree clean**

Run: `git status --porcelain`
Expected: empty (every artifact of every task committed). If a fix was needed in Steps 1–5, commit it as `chore(compile): workspace verification fixes`.

---

## Self-Review Record (kept with the plan per writing-plans)

- **Spec coverage:** §4.1 envelope → T2/T12; §4.2 three-id model → T2 (`code` skipped)/T11; §4.3 stamps → T2/T7/T9/T10; §4.4 all 14 record kinds → T2, lowering T7 (primitives), T8 (choice/match/jump), T9 (injected sprite forms), T10 (barrier); §4.5 worked examples → pinned in T2/T9/T14; §5 pass 1 → T12 gate, pass 2 → T6, pass 3 → T5, pass 4 → T7+T8, pass 5 → T9+T10, pass 6 → T11, pass 7 → T12/T13; §6 crate layout → File Structure (one file per spec module); §7 VM contract → T8 (flow/nesting), T9 (join, injected sprites); §8 testing → goldens per construct (T7/T8), injection incl. fork/join (T9), e2e + determinism (T14), gate (T12); §9 in-scope list → every item has a task (camera T7, dialogMotion/as T7, all set ops T7, when/persist T6/T8, otherwise omission T8, timeline stage-changing clips + barrier T10, ::use T6, @ref/@fn/$ T5, folded envelope T12, ids T11, CFG injection T9, CLI T13); D1–D9 → T2 (D1/D3), T8 (D2), T5 (D4), T7 (D5: assetId verbatim), T12 (D6), T1/T13 (D7), T6 (D8), T9/T10 (D9); §10 hygiene → Global Constraints + T15; §11 open questions honored (camera wait=false T7; conservative join T9; fold_env accessor T4; voiceKey = characterId-code T11).
- **Out-of-scope guard:** no asset-path resolution, no translation merge, no runtime CEL evaluation, no incremental compile anywhere in the plan (§9 Out).
- **Placeholder scan:** no TBD/TODO/"add error handling"/"similar to Task N"; the only non-inline code references are two VERBATIM-MOVE refactors (T4 `fold_env`, T13 `build_input`) with exact cut anchors and full replacement code — the moved lines exist in the repo and are pinned by their surrounding tests.
- **Type consistency:** `Command` variant/struct names, `Stamp`, `WalkCx`, `ClipStamp`, `Emitter::{fresh,bind,push,finish}`, `join_states`, `schedule_timeline`, `ShotRecords`/`IdCx`/`assign_addresses`, `DefTable`/`expand_cel`/`expand_document`, `normalize_document`/`cel_string_literal`/`COMPONENT_BEGIN`/`COMPONENT_END`, `FoldedEnv`/`fold_env` are spelled identically at every reference (producer + consumer tasks).
