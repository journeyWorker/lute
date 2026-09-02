# Lute 0.16.0 — Declarative Rewards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** `<reward kind= target= amount= when= on=/>` as a direct child of `<quest>`/`<objective>`, lowered to `QuestCmd.rewards`/`ObjectiveEntry.rewards` pure data, granted by engine rules, surfaced as deterministic grant events by the reference runtime, optionally vocabulary-checked via a plugin `rewardKinds:` export.

**Architecture:** Reward is an OWNER FIELD (`Quest.rewards: Vec<Reward>`, `Objective.rewards: Vec<Reward>`), never a `Node` variant (spec D-A) — the quest/objective body parse loops intercept self-closing `<reward>` and collect it; every existing exhaustive `Node` match stays untouched. `reward.when` joins the canonical CelSlot walk so fill/profile/defassign/LSP see it for free. Lowering is a pure vector map (no synthesis — inverse of 0.14.0 subquests). Runner/trace emit grant events at fresh transitions; ranges are never rolled (spec D-C).

**Tech Stack:** Rust workspace; tree-sitter grammar regen (`npx tree-sitter-cli`).

**Spec:** `docs/proposals/scenario-dsl/0.16.0.md` + `docs/superpowers/specs/2026-09-01-lute-reward-design.md`. Survey anchors: `agent://RewardScout` findings are inlined below per task.

## Global Constraints

- `export PATH="$HOME/.cargo/bin:$PATH"`; worktree authoritative `/Users/journey/Workspace/lute/.worktrees/lute-0.16.0` on `feat/lute-0.16.0-rewards`. HARNESS QUIRK: relative write/edit paths resolve against the MAIN workspace — ABSOLUTE worktree paths; cargo cwd = worktree. Cargo target dir is SHARED with the main workspace (`/Users/journey/Workspace/lute/target`) — never trust a worktree-local `./target`.
- TDD tester-first; own-crate `cargo test -p <crate>` during a task; full workspace gate at plan end only. `cargo fmt -p` + `cargo clippy -p <crate> --all-targets -- -D warnings` per touched crate before each commit.
- Wire contract (fixed here, do not renegotiate): `RewardEntry { kind: String, target: Option<String>, amount: Option<i64>, amount_min/amount_max: Option<i64> (serialized amountMin/amountMax), when: Option<CelPair>, on: Option<String> /* only "failed" ever serialized; objective entries never carry it */ }` — exactly one of `amount` XOR (`amountMin`+`amountMax`) present after defaulting (`amount: 1`).
- AST amount repr: `enum RewardAmount { Scalar(i64), Range(i64, i64) }` (spec §2: integers, negatives legal, `N <= M`).
- Diagnostics exactly: `E-REWARD-ATTR` (shape), `E-REWARD-KIND` (vocabulary); `when=` through the existing CEL registry; unknown attrs through the D-J per-tag table.
- Grant order (spec D-D): declaration order within owner; objective grants → quest grants → lifecycle handler bodies; at-most-once per instance; cascade-failure grants `on="failed"` exactly once.
- Version bump to 0.16.0 happens in Task 5 ONLY (constants, schema rename, conformance) — earlier tasks keep 0.15.1 so their test churn stays local.

---

### Task 1: Syntax — Reward AST + owner-field parsing + CEL walk

**Files:**
- Modify: `crates/lute-syntax/src/ast.rs:146-193` (add `Reward`, `RewardAmount`; `rewards: Vec<Reward>` on `Quest` AND `Objective`), `crates/lute-syntax/src/parser/blocks.rs:216-290` (quest/objective body loops intercept `<reward`), `crates/lute-syntax/src/parser.rs:399-425` (unknown-tag arm: `<reward>` outside an owner keeps today's rejection, message may name legal positions), `crates/lute-syntax/src/walk.rs:40-139,245-277` (visit `reward.when` as a Bool CelSlot: quest rewards after quest attrs, objective rewards after objective when/done, declaration order).
- Test: `#[cfg(test)]` in parser tests + walk tests (existing module layout).

**Interfaces (Produces):**
```rust
pub struct Reward {
    pub kind: String,            // may be empty when malformed — checker diagnoses
    pub kind_span: Span,
    pub target: Option<String>,
    pub amount: Option<RewardAmount>,  // None = unauthored (defaults later)
    pub amount_span: Option<Span>,
    pub when: Option<CelString>,       // same repr objective.when uses
    pub on: Option<String>,            // raw value; checker validates the enum
    pub on_span: Option<Span>,
    pub attrs: Vec<Attr>,              // residual attrs for the D-J closure check
    pub span: Span,
    pub self_closing: bool,            // parser accepts only true; body form -> parse error
}
pub enum RewardAmount { Scalar(i64), Range(i64, i64) }
```
Parsing rules: `parse_open_tag` supplies one-line + self-closing detection; a non-self-closing `<reward>` is a parse-layer error (reuse the closest existing parse diagnostic shape for a body on a leaf; do NOT invent a recovery body scan). `amount` parses `-?\d+` or `-?\d+\.\.-?\d+`; a malformed literal keeps `amount: None` + the raw attr in `attrs` so the checker can anchor `E-REWARD-ATTR` (parser stays diagnostic-light; the checker owns the code).

- [ ] **Step 1: failing tests** — quest-level reward parses into `quest.rewards` (kind/target/amount scalar); range literal parses to `Range(1,5)`; negative scalar; objective-level reward into `objective.rewards`; `<reward>` in a scene body → today's unknown-tag error; non-self-closing `<reward>` → error; walk visits `reward.when` slots in declaration order (extend the existing walk-order test).
- [ ] **Step 2: RED** — `cargo test -p lute-syntax`.
- [ ] **Step 3: implement**; **Step 4: GREEN**; **Step 5: fmt+clippy+commit** — `feat(syntax): <reward/> owner-field parsing + RewardAmount + CEL-slot walk (dsl 0.16.0 §2)`.

### Task 2: Checker — shape, closure, CEL, vocabulary

**Files:**
- Modify: `crates/lute-check/src/logic_attrs.rs:92-203` (REWARD_ATTRS = `[kind, target, amount, when, on]` row + explicit call for every reward), `crates/lute-check/src/match_check.rs:558-810` (`check_quest` gains reward checks: empty kind, malformed/unparsed amount attr, `N > M` already unrepresentable (parser), `on` enum + objective-level `on` rejection → `E-REWARD-ATTR`; vocabulary → `E-REWARD-KIND` when the resolved snapshot carries reward kinds), `crates/lute-check/src/check.rs:1494-1524` (Walker: reward.when Bool profile check via `check_cel_slot`, mirroring objective.when), `crates/lute-check/src/defassign.rs:472-515` (reward.when joins the objective-when-style slot-local `E-MAYBE-UNSET` modeling).
- Test: crate test files beside the anchors.

**Interfaces:** Consumes Task 1 AST. Produces: diagnostics per Global Constraints; `snapshot.reward_kinds: BTreeMap<String, RewardKindDecl>` READ here (empty map = shape-only) — the type arrives in Task 4; until then gate on an `Option`/empty default so Task 2 lands independently (define the field in lute-manifest in THIS task as an empty-by-default `BTreeMap` with no loader — Task 4 populates it).

- [ ] Failing tests: empty kind → E-REWARD-ATTR; `amount="x"`/`amount="5..2"` → E-REWARD-ATTR anchored at amount; objective reward with `on="failed"` → E-REWARD-ATTR; `on="banana"` → E-REWARD-ATTR; unknown attr `foo=` → E-UNKNOWN-ATTR; `when="run.x >"` → existing CEL parse code; `when` reading a maybe-unset path → E-MAYBE-UNSET; with a snapshot carrying `rewardKinds {SHARD}`: `kind="GOLD"` → E-REWARD-KIND, `kind="SHARD"` clean.
- [ ] RED → implement → GREEN (`cargo test -p lute-check -p lute-manifest`) → fmt+clippy+commit — `feat(check): reward shape/closure/CEL/vocabulary checks (dsl 0.16.0 §2,§4,§6)`.

### Task 3: Compile — RewardEntry lowering (pure vectors)

**Files:**
- Modify: `crates/lute-compile/src/ir.rs:823-924` (add `RewardEntry` per the wire contract; `rewards` on `QuestCmd` + `ObjectiveEntry`, `skip_serializing_if = Vec::is_empty`), `crates/lute-compile/src/stage.rs:604-671` (`walk_quest` pass 1 maps owner vectors; `when` via `CelPair::from_raw`; amount defaulting `None → amount: 1`; NO handler/command synthesis), `crates/lute-compile/src/normalize.rs` untouched (assert via test that subquest synthesis does not touch rewards).
- Test: `crates/lute-compile/tests/compile.rs` + e2e snapshot for a reward-carrying quest.

- [ ] Failing tests: quest+objective rewards serialize in declaration order with exact JSON field names (`amountMin`/`amountMax` for ranges, `on` only when `"failed"`, `when.raw` verbatim); rewardless quest artifact byte-identical to pre-change (serde Value diff); subquest `quest=` synthesis leaves both parties' rewards untouched.
- [ ] RED → implement → GREEN (`cargo test -p lute-compile`) → fmt+clippy+commit — `feat(compile): RewardEntry on QuestCmd/ObjectiveEntry — pure data lowering (dsl 0.16.0 §3,§5)`.

### Task 4: Manifest — `rewardKinds:` export + guarded capability hash

**Files:**
- Modify: `crates/lute-manifest/src/schema.rs:271-324` (`RewardKindsFile`/`RewardKindDecl { target: Option<TargetContract>, attrs: … }` following `EventsFile`), `src/loader.rs:125-260` (closed export-name match + duplicate check + UnknownExport message), `src/assemble.rs:178-460` (owner-tracked merge; reject a kind whose named provider/domain is absent), `src/snapshot.rs:14-250` (sorted map + GUARDED hash section: empty ⇒ hash byte-identical — assert against the pinned core `capabilityVersion`).
- Test: `crates/lute-manifest/tests/loader.rs` + `tests/assemble.rs` following the events/stampAttrs fixtures; the tree-sitter stamp drift-guard test MUST stay green untouched.

- [ ] Failing tests: plugin declaring `rewardKinds: {SHARD: {}, ITEM: {target: {provider: item}}}` loads + assembles; duplicate kind across plugins → existing duplicate diagnostic shape; kind naming an absent provider → assembly error; capabilityVersion with NO rewardKinds == pinned current hash (byte-stability guard); WITH rewardKinds ≠ (moves).
- [ ] RED → implement → GREEN (`cargo test -p lute-manifest`, then `cargo test -p lute-check` reward-vocabulary tests flip to the real loader) → fmt+clippy+commit — `feat(manifest): rewardKinds plugin export, guarded capability-hash section (dsl 0.16.0 §4)`.

### Task 5: Runtime + version + schema + conformance

**Files:**
- Modify: `crates/lute-cli/src/runner.rs:1331-1626` (parse reward arrays from artifact JSON; emit grant transcript events: objective grants at fresh done, quest grants at fresh complete/failed incl. `cascade_children`, order per spec D-D, `when` evaluated at grant instant, ranges carried verbatim; human + `--json` renderings), `crates/lute-trace/src/walk.rs:1037-1445` + `src/report.rs:68-112` (`Step::Grant` at the same fresh transitions + human line), version constants (`lute-check/src/lib.rs`, `lute-compile/src/lib.rs` → `0.16.0`, workspace `Cargo.toml`, alignment test rename), schema rename `lute-ir-0.15.schema.json → lute-ir-0.16.schema.json` (+ `rewardEntry`, both arrays, version pins; per-release rename rule), `conformance/README.md` link + grant-event doc, conformance regeneration: extend `quest-complete/source.lute` with a quest reward + an objective reward and `quest-subquest/source.lute` with a child-failure `on="failed"` case, regenerate all 7 fixtures via the rebuilt binary.
- Test: `crates/lute-trace/tests/quest.rs` (grant timing/order/once/when/range), `crates/lute-cli` conformance suite.

Grant event JSON shape (transcript): `{ "kind": "grant", "quest": "<id>", "objective": "<oid>"?, "reward": { …RewardEntry sans when… }, "onFailed": true? }` — deterministic, range bounds verbatim, never a rolled value.

- [ ] Failing tests first (trace quest suite): objective grant fires once at first done, before quest grant, before `questComplete` handler content; `when=false` skips exactly that reward; `on="failed"` grants on fail AND on cascade-fail, never on complete; range event carries min/max.
- [ ] RED → implement → regenerate conformance → GREEN (`cargo test -p lute-trace -p lute-cli -p lute-compile -p lute-check`) → fmt+clippy+commit — `feat(runtime): reward grant events in runner/trace; IR 0.16.0 + schema + conformance`.

### Task 6: tree-sitter grammar + docs

**Files:**
- Modify: `tree-sitter-lute/grammar.js:38-72,246-280` (self-closing `reward` production reachable in quest AND objective bodies — owner-scoped, not `_node`), `queries/highlights.scm:77-80` (reward capture), `test/corpus/quest.txt` (valid quest-level/objective-level parses + a non-self-closing negative), regenerate artifacts (`npx --yes tree-sitter-cli@latest generate && npx --yes tree-sitter-cli@latest test`). `capabilityVersion` stamps MUST NOT change (they track the core snapshot; Task 4's guarded section keeps the hash stable).
- Docs: `docs/runtime/quest-lifecycle.md` (grant engine rules per spec §3, 0.14.0-section template), `docs/getting-started-first-scene.md` untouched; `docs/examples/quest-grove.lute` gains quest complete/failed + objective rewards; `docs/examples/quest-subquest.lute` gains one ownership-demonstrating reward; `docs/adoption/oshiz-assessment.md:257-279` gains a short cross-reference note (declarative form now covers the data half; do not rewrite history).

- [ ] tree-sitter corpus green; grammar artifacts regenerated; stamp drift-guard test green.
- [ ] Docs updated; example docs `lute check` clean via the rebuilt binary.
- [ ] fmt (none — js/md) + commit — `feat(grammar),docs: reward production + engine rules + example coverage`.

### Task 7: Final gate (plan owner)

- [ ] `cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -D warnings` 0; tree-sitter suite green with unchanged capabilityVersion stamps.
- [ ] Corpus: `check-project docs/examples/anseo` exit 0 unchanged; rewardless artifact byte-diff = version strings only (baseline vs branch, scripted JSON diff).
- [ ] Smoke: scratch quest with all five reward shapes (scalar/negative/range/when/on=failed + objective reward) → check 0 → compile → `lute run` transcript shows ordered grant events → `lute trace` mirrors them.
- [ ] Whole-branch review → merge decision with user.

## Self-Review

- **Spec coverage:** §2 (T1 parse + T2 shape/closure), §3 (T3 lowering + T5 engine/runtime + T6 lifecycle docs), §4 (T4 + T2 vocabulary read), §5 (T3 + T5 version/schema), §6 (T2), §7 (T5 conformance + T7 corpus gate + T6 stamp guard). D-A..D-G each land in T1/T3/T5/T5/T4/T5(D-F: transcript only)/T2(D-G: no chance attr).
- **Placeholder scan:** parse-layer body-on-leaf diagnostic reuses "the closest existing parse diagnostic shape" — implementer picks the exact code in-file (parser owns several; naming one blind would be wrong).
- **Type consistency:** `Reward`/`RewardAmount` (T1) consumed by T2/T3; `RewardEntry` JSON names fixed in Global Constraints and repeated in T3/T5; `snapshot.reward_kinds` bootstrap (T2 empty default → T4 real loader) is explicit.
