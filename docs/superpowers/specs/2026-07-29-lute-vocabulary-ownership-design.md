# Lute — Vocabulary Ownership: Completing the Data-Catalog Split (approved design)

- **Date:** 2026-07-29
- **Status:** approved design; spec-first (documents/decisions before implementation)
- **Version:** **language 0.9.0** + **plugin-system 0.0.3** delta. Breaking at the language axis
  (pre-1.0 allowance, dsl 0.1.0 §2) in exactly one narrow way (D-D); **IR schema unchanged at 0.8.0**
  by construction (D-B).
- **Drives:** `docs/proposals/scenario-dsl/0.9.0.md` (new), `docs/proposals/plugin-system/0.0.3.md`
  (new delta), `crates/lute-manifest` (`core.rs` split, embedded `lute.vn`, `enums:` shape,
  `sceneSlot:`, `SEMANTICS_VOCAB`, `overrides:` merge), `crates/lute-check`
  (`content_line.rs` guard removal, `inject.rs` flag-driven reducer, `schema_import.rs`
  `merge_domains`), `crates/lute-compile` (`stage.rs` scene-slot join), `docs/plugin-system.md`,
  `docs/adoption/oshiz-assessment.md` (D1 row correction), shipped JSON Schemas.
- **Provenance:** this session's diagnosis dialogue. **This is not a new direction.** It completes
  `2026-07-10-lute-data-catalog-foundation-design.md` **D1**, whose approved classification the
  implementation silently deviated from, and refines D1's litmus so that "fixed core" shrinks to
  almost nothing.

---

## 0. The problem, stated as a deviation (normative framing)

Data-catalog foundation **D1** classified every domain into two buckets with an explicit litmus:

> **Fixed core enums** — the compiler branches on the specific member, so the members ARE language
> semantics. These live in `lute.core`, are closed, and are **NOT author-extensible**.
> **Data vocabularies** — the checker only membership-checks; the value flows through as data
> (`emotion`, `action`, `character`, `costume`, `mood`, `vfxType`). These are **author/plugin
> definable**.
> (Litmus: does the compiler/engine change behavior by *which member* it is?)

`emotion`, `mood`, and `vfxType` were classified **definable**. The implementation put all six
baseline enums into `lute.core` as `open: false` and made every merge path drop-and-report, so
**none of them is definable by anyone**:

| Path | Result today | Site |
|---|---|---|
| project schema `enums: emotion: […]` | `E-DOMAIN-DUP`, project members dropped | `schema_import.rs:508` |
| own capability plugin exporting `emotion` | `E-PLUGIN-DUP-ACROSS` + `E-DOMAIN-DUP`, **whole project resolution fails** | `assemble.rs:308` → `merge_map:462` |
| provider named `emotion` | closed domain wins over same-named provider | `directives.rs:390` |
| not activating `lute.core` | impossible — unconditional | `resolve.rs:141` |
| project-schema `extends` "may only ADD members" relaxation | exists only *within* the schema-file chain; core is not a base of that chain | `schema_import.rs:399` |

Two further documents already stand on the other side of this: `docs/plugin-system.md:57` names
`emotion="smug"` as an example of *registrable data*, and the D1 quote above. The profile machinery
the split needs **already ships** — `lute.project.yaml` `profiles:`/`defaultProfile:`/`extends:`,
scene-local activation, and the six-step resolution order (`project.rs:62`, `resolve.rs:108`). The
only thing missing is that the content vocabulary sits inside the one plugin that cannot be
influenced.

**Measured cost of the deviation** (`eevee/packages/data-catalog`, OSHiZ):

- `emotion`: 30,861 authored values / **17 distinct**; `lute.core` admits 7, of which 6 are used.
  **6,377 occurrences (20.7%) are unrepresentable**, and core's `angry` is never used — OSHiZ says
  `furious`. Asset ids repeat the same split (`CH.{character}.{costume}.{emotion}.{variant}`, 38,090
  rows).
- `mood`: 10 distinct, 2 overlap. (Non-blocking today: `::music{mood}` is `type: string`,
  `staging.yaml:12` — the `mood` domain has no consumer.)
- `anchor`: 3/3 exact match. No bite.
- `action`: 9,880 authored values / **53 distinct**, and today they receive **zero** validation
  (see D-D). A typo ships.

## 1. Non-goals

- **Directive override.** OSHiZ's measured need is *additive* (`foreground` 430, `external` 366,
  `split-screen-*` 18, `ui` 13, `love-lockdown-*` 60 alongside 7 of core's 8). Replacing a baseline
  directive is not designed here.
- **Partial member extension** (`emotion: +[affection]`). Override is whole-domain replacement. Two
  coexisting widening syntaxes would make "where does this member come from" unreadable.
- **Splitting content-line `action` from `::auto{action}`** into two domains. They are conflated
  today and stay one domain (`action`); in practice members overlap (`sway`/`lean`/`nod` are both
  pose and transition ids). Revisit only on evidence.
- **Registry-style (`open: engine`) baseline domains.** Baseline domains stay closed.

---

## D-A. Split `lute.core` along "does the compiler dispatch on the name?"

| Plugin | Owns | Why |
|---|---|---|
| **`lute.core`** — language, embedded, cannot be deactivated | `::end` | `terminatesWalk`, but the compiler lowers a terminator **by tag**: `lute-check` (`W-CODE-AFTER-END`, `<track>` clip guard), `lute-compile` (`Command::End`), `lute-trace` all dispatch on the name (`core.rs:17-24`) |
| **`lute.vn`** — baseline profile, embedded, **auto-activated**, **overridable** | `::bg ::music ::sfx ::auto ::vfx ::cut ::video ::camera` + domains `emotion mood volume anchor vfxType musicAction action` | after D-F nothing dispatches on these names |

Grammar (`@speaker{}:`, `<branch> <choice> <match> <when> <quest> <track>`, delivery flags
`mono/os/vo`) is not in any manifest and is untouched.

**`lute.vn` is embedded (`include_str!`) and auto-activated exactly like `lute.core`.** Consequently
`lute check <file>` with no project resolves byte-identically to today, and every golden,
conformance fixture, README snippet, and `docs/` example keeps working unchanged. The split moves
files and ownership; it removes no capability.

`lute.vn` gains one domain that `lute.core` never declared — `action` — because D-F needs it
(rationale in D-E). Its members are **recorded, not invented**: see D-E.

## D-B. The attribute name is the language's; the value set is the plugin's

`content_line.rs:23 KNOWN_ATTRS` and its 1:1 mapping to IR fields (`lower.rs:43`) **do not move**.
`emotion=` stays a content-line grammar slot that lowers to a top-level IR field; only the *domain
it resolves against* is plugin-owned and late-bound.

```jsonc
// unchanged, guaranteed by this decision
{ "role":"dialogue", "text":"Mr. Fixer!", "emotion":"delighted", "variant":1, … }
```

Rejected alternative: moving `emotion`/`action`/`variant`/`dialogMotion` into `lute.vn`'s
`stampAttrs`. It is more uniform, and the mechanism already admits it
(`content_line.rs:79` types a `stampAttrs` key through the same resolver), but a `stampAttrs` key
lowers into the record's `stamp` rather than a named field. That is an IR break for every consumer
and 30,861 OSHiZ rows, bought for purity. Rejected. `irVersion` therefore stays `0.8.0`.

## D-C. Override is per-domain and declared by the replacing side

```yaml
# plugins/oshiz.vn/enums.yaml
overrides: [emotion, mood, action]     # replace these baseline domains wholesale
enums:
  emotion: [neutral, content, delighted, shy, surprised, sad, affection, puzzled,
            anxious, worried, furious, contempt, serious, embarrassed, smile, happy, bright]
  mood:    [cheerful, sentimental, peaceful, nervous, romantic, mysterious, quirky,
            sadness, action, gag]
  action:
    members: [fade-in, sway, bounce, lean, hide, nod, hop, step-forward, …]
    exits:   [fade-out, fade-out-down, fade-out-slow, hide]
```

A `.lute` project schema is symmetric: `overrides:` is a frontmatter key beside `enums:`.

Rules:

1. **Only a baseline-owned domain may be overridden.** Baseline = the embedded plugins
   (`lute.core`, `lute.vn`) — a closed two-element set the code already knows, so no new manifest
   key. Overriding a peer plugin's domain → `E-DOMAIN-OVERRIDE-PEER`.
2. Two sources overriding the same domain → `E-DOMAIN-OVERRIDE-CONFLICT`.
3. Overriding a name no baseline declares → `E-DOMAIN-OVERRIDE-UNKNOWN`. Never silently demoted to
   an ordinary declaration; an unlisted name is a typo.
4. Re-declaring a baseline name **without** `overrides:` keeps today's `E-DOMAIN-DUP`. Implicit
   shadowing stays forbidden — this preserves the foundation design's "never a silent shadow".
5. An override **replaces** `members` (and `default`/`exits`, D-E) wholesale. It does not merge.
6. `lute.vn` staying activated means non-overridden domains and all eight directives survive an
   override — the reason OSHiZ does not have to re-declare 7 directives it uses unchanged.

**Cosmetic fix in the same change:** `merge_map` hardcodes `first: "?"` (`assemble.rs:466`), so the
current message reads ``declared by both `?` and `oshiz.vn` ``. Thread the owning plugin id.

## D-D. A domain slot with no declared domain is an error

`content_line.rs:161` currently *skips* validation when nothing declares `action`:

```rust
"action" if (domains.contains_key("action") || snapshot.providers.contains_key("action")) => { … }
_ => {}   // nobody declared it → silently clean
```

**Delete the guard clause.** `action` then falls through to `check_domain_member` exactly as
`emotion` does, and step 4 (`directives.rs:402`) already emits the correct diagnostic:

> `E-DOMAIN-UNKNOWN` — `action` is not a known domain, declared by neither the plugin/core
> vocabulary nor a project schema

**No new diagnostic code, and strict behavior arrives by removing a special case, not by adding a
rule.** Reword the message for the content-line context; keep the code.

Scope: content-line domain slots (`emotion`, `action`). A *directive* attr declared
`type: string` is the plugin author's explicit opt-out and is not touched — a plugin wanting the
check writes `{ domain: X }`. `dialogMotion` is not a domain slot today and does not become one.

Breakage: a document using content-line `action=` in a project that declares **no** `action` domain.
Because `lute.vn` declares one (D-E), this is empty for any baseline project. Measured surface:
content-line `action=` appears **twice in the whole repo** — `action="sway"` and `action="lean"` in
`docs/architecture.md:162,165` — and both values are baseline members by D-E(b). Zero `.lute` files
use it; the only other occurrences are Rust unit tests that build source strings inline
(`inject.rs:94` `action="pose-lean"`, `content_line.rs:67` `action="wave"`), and `pose-lean` is the
one value that must be updated to a member when its test is touched.

## D-E. `enums:` grows a long form: `members` / `default` / `exits`

Applying D1's litmus to today's code shows `action` and `anchor` are *fixed core* — the compiler
branches on **which member**:

| Hardcoded member semantics | Site |
|---|---|
| `is_exit_action`: `fade-out*` \| `exit*` \| `hide` exits a character | `inject.rs:370` |
| `DEFAULT_ANCHOR = "center"`, plus a warning for a redundant explicit `center` | `inject.rs:61,229` |

A directive-level `semantics` flag cannot express either — both are properties of *individual
members*. So the domain declaration carries them:

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]   # flat list stays legal
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fade-in-up, fade-in-slow, slide-in-left, walk-in, idle, wave, pose-turn,
              sway, lean, fade-out, fade-out-down, fade-out-slow, hide]
    exits:   [fade-out, fade-out-down, fade-out-slow, hide]
```

- A flat list is shorthand for `{ members: […] }`. Every existing `enums.yaml` keeps parsing.
- `default:` must be a member → else `E-ENUM-DEFAULT-NOT-MEMBER`.
- Every `exits:` entry must be a member → else `E-ENUM-EXITS-NOT-MEMBER`.
- `exits:` is meaningful only where the domain is bound to a `mayExitCharacter` directive attr
  (D-F); declaring it elsewhere is inert, not an error.

**`lute.vn`'s baseline `action` members are recorded from evidence, never invented.** Appendix A
(`scenario-dsl/0.0.1.md:577`) deliberately left `::auto{action}` an open action-id
(*"e.g. `fade-in-up`/`fade-out-down`/`pose-*`"*), so there is no prior enumeration to copy. The
member list above is exactly the union of three measured sets:

- (a) every `::auto{action="…"}` value in the repo — `fade-in-up` ×22, `slide-in-left` ×3,
  `fade-out-down` ×2, `fade-in-slow`, `idle`, `pose-turn`, `walk-in`, `wave`;
- (b) every **content-line** `action="…"` value — `sway` and `lean`, both in `docs/architecture.md`
  (`:162`, `:165`). CI would **not** catch their omission: the `examples` job runs
  `check-project docs/examples` only, and the `website` job checks syntax highlighting, not
  semantics (`.github/workflows/docs.yml:70,73`). They are included because shipped reference
  documentation must remain valid Lute, not because a gate forces it — and their absence from any
  gate is itself worth noting in the docs task (§5.7);
- (c) the three exit ids `is_exit_action` recognizes that no example happens to use — `fade-out`,
  `fade-out-slow`, `hide`.

`exits:` reproduces `is_exit_action`'s verdict on every member, so behavior is preserved and every
golden holds. `::auto{action}` is retyped `string` → `{ domain: action }`.

Only `::auto` is retyped. `::cut{action}` and `::video{action}` are already `{ enum: [show, hide] }`
(`staging.yaml:40,46`) and `::music{action}` is already `{ domain: musicAction }` (`:11`) — three
attrs that share the name `action` and are deliberately left alone.

## D-F. Wire the semantics flags; delete the fiction

`SEMANTICS_VOCAB` (`validate.rs:4`) declares 11 flags. **Zero are consumed by the checker or
compiler.** `isExit`/`isStateful` were named for `inject.rs`'s `is_exit_action`/`line_is_stateful`
and are attached to no directive; `usesAnchor`/`mayExitCharacter`/`reads.onStage`/
`writes.characterState` appear in `inject.rs` **only inside a comment** (`:37-38`). The module
header has acknowledged this since Task 4.8 (`:39-44`).

`mutatesScene` cannot drive the dispatch it looks like it should: **it is declared on both `::bg`
and `::music`** (`staging.yaml:7,16`), so branching on it would make `::music` clear the stage and
auto-hide every sprite.

| Flag | Disposition | Consumer after this change |
|---|---|---|
| `clearsStage` | **new**, on `::bg` only | reducer: auto-hide all on-stage sprites, clear `on_stage`/`dirty` (today `d.tag == "bg"`) |
| `writes.characterState` | wire | reducer: this directive participates in stage bookkeeping (today `d.tag == "auto"`) |
| `usesAnchor` | wire | rule `auto-anchor-on-show` |
| `mayExitCharacter` | wire | consult the bound domain's `exits:` (D-E) instead of `is_exit_action` |
| `isExit` | wire | directive **always** exits its character, independent of the action member |
| `requiresAnchor` | wire | new `E-ANCHOR-REQUIRED` when the anchor attr is absent |
| `mutatesScene` | keep, re-scoped | record scene state into the declared `sceneSlot` (below) |
| `writes.sceneState`, `bridgeCall`, `reads.onStage`, `terminatesWalk` | unchanged | existing roles |
| `isStateful`, `cancelsPrevious` | **remove from the vocabulary** | no honest consumer; a closed vocabulary must not carry fiction. Safe: no shipped directive declares either (plugin 0.0.3 delta) |

The last name-dependency is *which* scene-state slot a directive writes and from which attrs
(`state.bg = location｜assetId`, `state.music = mood｜action`, `inject.rs:147,318`). Declare it:

```yaml
- name: bg
  sceneSlot: { name: bg, from: [location, assetId] }
  semantics: [ mutatesScene, clearsStage ]
- name: music
  sceneSlot: { name: music, from: [mood, action] }
  semantics: [ mutatesScene ]
```

`from:` is an ordered fallback list — the **first attr present on the node wins**, reproducing
today's `attr_str(…, "location").or_else(… "assetId")` exactly. A `sceneSlot` whose `name` collides
with another active plugin's slot is an `E-DOMAIN-DUP`-class cross-plugin duplicate, resolved by the
same first-owner-wins `merge_map` every other snapshot map uses.

Two conditions must both hold for a directive to participate in **character** bookkeeping under
`writes.characterState`: the flag, **and** a declared attr the reducer can read the character from.
`lute.vn`'s `::auto` has `character` (`staging.yaml:26`); a plugin flagging a directive with no such
attr gets `E-CHARACTER-ATTR-MISSING` at assembly, not a silent no-op. This keeps the reducer total
without reintroducing name knowledge.

`StageState`'s two scalar slots become `scene: BTreeMap<String, Option<String>>`. `lute-compile`'s
branch-exit join (`stage.rs:517`, the only external reader of `StageState::music`) becomes a
map-wise comparison. After this, `lower_node`'s `match` has no `d.tag == …` arm.

**Content-line statefulness** (`line_is_stateful:360`) stays core-owned — it keys off the
core-owned `KNOWN_ATTRS` slots per D-B — with the dead entry removed (D-G).

## D-G. Deletions

- **`pose` is dead.** `inject.rs:350` reads a `pose` attr and `:364` lists it as sprite-affecting,
  but `pose` is absent from `KNOWN_ATTRS` (`content_line.rs:24`), so `@x{pose="…"}` is
  `E-UNKNOWN-ATTR` and neither line is reachable. Remove both; the stateful set becomes
  `emotion | variant | action | dialogMotion`.
- `is_exit_action` (`inject.rs:370`) — superseded by `exits:`.
- `DEFAULT_ANCHOR` (`inject.rs:61`, re-exported at `lib.rs:63`) — superseded by `default:`.
- `isStateful`, `cancelsPrevious` from `SEMANTICS_VOCAB`.

---

## 2. Diagnostics

| Code | Severity | Meaning | New? |
|---|---|---|---|
| `E-DOMAIN-UNKNOWN` | error | domain slot used, no source declares the domain (D-D) | reused; message reworded |
| `E-DOMAIN-DUP` | error | baseline name re-declared without `overrides:` (D-C.4) | existing; owner id fixed |
| `E-DOMAIN-OVERRIDE-PEER` | error | override targets a non-baseline plugin's domain | ✔ |
| `E-DOMAIN-OVERRIDE-CONFLICT` | error | two sources override the same domain | ✔ |
| `E-DOMAIN-OVERRIDE-UNKNOWN` | error | override targets a name no baseline declares | ✔ |
| `E-ENUM-DEFAULT-NOT-MEMBER` | error | `default:` is not in `members:` | ✔ |
| `E-ENUM-EXITS-NOT-MEMBER` | error | an `exits:` entry is not in `members:` | ✔ |
| `E-ANCHOR-REQUIRED` | error | `requiresAnchor` directive without an anchor attr | ✔ |
| `E-CHARACTER-ATTR-MISSING` | error | `writes.characterState` on a directive with no character attr (D-F) | ✔ |

## 3. Compatibility

| Axis | Effect |
|---|---|
| **IR schema** | **unchanged (0.8.0)** by D-B. No artifact field moves |
| **capabilityVersion** | changes — the core snapshot splits and `lute.vn` joins. Stamped and expected (plugin §13) |
| **Existing documents** | unchanged: `lute.vn` is auto-activated and declares every baseline domain including `action`. The only strictness-sensitive surface is content-line `action=`, which appears twice repo-wide (`docs/architecture.md:162,165`) and whose two values are baseline members (D-E) |
| **Existing plugins** | break only if one declares `isStateful`/`cancelsPrevious` (none shipped) |
| **`lute.project.yaml`** | no new keys. Override lives in the vocabulary declaration, not the profile |
| **Versions** | language 0.9.0; plugin-system 0.0.3; toolchain per release |

## 4. Verification plan

Evidence, not assertions — each item names the command that produces it.

1. **Baseline is byte-identical.** `cargo test -p lute-check -p lute-compile` with **no golden or
   snapshot re-recording**. If any `crates/*/tests/snapshots/*.snap` changes, D-A or D-E is wrong.
   The one expected test edit is `core_snapshot_has_baseline_directives` (`core.rs:96`, asserts
   exactly 9 directives), which splits into a `lute.core` assertion (`::end`) and a `lute.vn`
   assertion (the other 8).
2. **`exits:` reproduces `is_exit_action`.** A table test over all 13 baseline `action` members
   asserting `exits:` membership equals `is_exit_action`'s verdict, written and passing **before**
   the function is deleted, so the replacement is proven equivalent rather than assumed.
3. **Override works end to end.** The `plugins/oshiz.vn/` fixture used in this session's diagnosis,
   now with `overrides: [emotion]` and OSHiZ's 17 members: `lute check scene.lute --project .`
   accepts `@bianca{emotion="affection"}: hi` and still rejects `emotion="zzz"`, and
   `lute context scene.lute --project . --json | jq .enums.emotion` returns the 17.
4. **Strict bites.** In a project declaring no `action` domain, `@x{action="wave"}` reports
   `E-DOMAIN-UNKNOWN`; declaring the domain makes it clean and `action="zzz"` `E-BAD-ENUM`.
5. **Override guards.** One fixture per code in §2 (`…-PEER`, `…-CONFLICT`, `…-UNKNOWN`,
   `E-ENUM-DEFAULT-NOT-MEMBER`, `E-ENUM-EXITS-NOT-MEMBER`).
6. **Flag-driven reducer is name-blind.** A fixture plugin declaring `::backdrop` with
   `clearsStage` + a `sceneSlot` gets auto-hide and scene-slot recording with **no** entry in
   `lute.vn`; conversely `::music` (`mutatesScene`, no `clearsStage`) must **not** clear the stage —
   the regression `mutatesScene`-dispatch would have caused.
7. **Real-data smoke.** Generate `plugins/oshiz.vn/enums.yaml` from
   `eevee/packages/data-catalog` (emotion 17, action 53, mood 10) and `lute check-project` a scene
   converted from `idola_script_commands/`; every previously-unrepresentable value must resolve.
8. **`lute doctor`** reports the active baseline plugins and which domains are overridden, so a
   mis-set profile is visible without reading YAML.

## 5. Sequencing

Ordered by strict dependency; each step ends green.

1. `enums:` long form + validation (`E-ENUM-DEFAULT-NOT-MEMBER`, `E-ENUM-EXITS-NOT-MEMBER`) —
   flat-list shorthand keeps every fixture passing.
2. `sceneSlot:` declaration + `StageState.scene` map + `stage.rs` join.
3. `SEMANTICS_VOCAB` edit (`+clearsStage`, `−isStateful`, `−cancelsPrevious`) and the flag wiring in
   `inject.rs`; delete `is_exit_action`, `DEFAULT_ANCHOR`, dead `pose`. Verification 2 and 6 gate it.
4. Split `core.rs` into `lute.core` + embedded auto-activated `lute.vn`, moving 8 directives and
   declaring the 7 domains. Verification 1 gates it.
5. `overrides:` in the plugin loader and `merge_domains`, plus the three override codes and the
   `first: "?"` owner fix. Verification 3 and 5 gate it.
6. Delete the `content_line.rs:161` guard; reword `E-DOMAIN-UNKNOWN`. Verification 4 gates it.
7. Docs: `scenario-dsl/0.9.0.md`, `plugin-system/0.0.3.md`, `plugin-system.md` rationale,
   `oshiz-assessment.md` D1 row (its `emotion` claim is wrong today), CHANGELOG.

Steps 1–3 are independent of 4–6 and may run in parallel; 5 depends on 4, and 6 depends on 5.
