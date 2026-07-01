# Design — Character identity, display label, costume, reveal & voice

- **Date:** 2026-06-30
- **Status:** Approved design (pre-implementation). Feeds `writing-plans`.
- **Owner surface:** Lute Scenario DSL — a new **character/cast capability plugin** plus small
  amendments to the language proposal and the plugin-system proposal.
- **Related specs:** [`../scenario-dsl/0.0.1.md`](../scenario-dsl/0.0.1.md)
  (language), [`../plugin-system/0.0.1.md`](../plugin-system/0.0.1.md)
  (plugins), [`../../architecture.md`](../../architecture.md) (compiler/StageState).

## 1. Problem

The bard inline compiler overloads a single `:line[...]` bracket to carry three unrelated
concepts, using delimiters, and mis-names the fields. Grounded in code:

- `packages/lute-core/src/modules/scenario/inline/parser.ts:801-808` — `:line[???-bianca]` is split
  at the **last hyphen** into `speaker="???"` (the dialog-box label) and `display="bianca"` (the
  **asset character id**). So the field called `display` is actually **identity**, not a display name.
- `.../inline/generator.ts:1049-1055` — `resolveCharacterId` returns `row.display` first; the
  compiled row (`generator.ts:336,385`) emits `speaker = row.speaker` (label) and
  `assetName = characterId` — so the runtime already separates label from identity; only the
  authoring surface conflates them.
- Character *variants* are faked as **separate ids** via an underscore suffix (`ann_child`,
  `nera_formal`), even though the asset system already has a first-class **costume** dimension:
  `resolveCharacterSprite` / `composeChAssetId` produce `CH.{characterId}.{costume}.{emotion}.{variant}`
  (`asset-catalog.ts:238-266`, `asset/canonical.ts` fallback chain). The line layer never sets costume.
- `.../inline/minter.ts` independently re-implements the last-hyphen rule (`resolveBracketCharacter`)
  to derive lineId ordinals — so any change touches parser **and** minter (and formatter, tests,
  prompt standards).
- `:line[???]` with no id (archived bianca S01EP01) collapses `speaker=display="???"` → asset lookup
  on `"???"` → broken. A `-`/`_` inside an id is likewise fragile.
- [INFERENCE, from code + review] dialogue sprite ids are built without a costume argument
  (`buildSpriteAssetId(row.character, emotion, catalog, variant)`), so a staged costume would not
  apply to subsequent dialogue lines — they snap back to the default costume.

Authored usage is widespread: `???-{id}` across ann/bianca/eris/nera scenarios and the
`prompts/scenario-formatter.md` / `scenario-standard-writer.md` standards; `ann_child`/`nera_formal`
for age/outfit variants.

**Three concepts to separate:** (1) **characterId** — identity (sprite/voice/state key); (2)
**speaker label** — what the dialog box shows (`???`, canonical name, role); (3) **costume** —
appearance variant of the same identity. Plus a fourth resolution output that must stay stable:
(4) **voiceKey** — the voice/line-join key.

## 2. Goals / non-goals

**Goals**
- Bracket carries **only** identity; no delimiter overloading; ids never break on `-`/`_`/`???`.
- Model `???` → real-name **reveal** as first-class state, DRY (flip once, all later lines switch),
  with a per-line override escape hatch.
- Model **costume** as a real, persistent, per-character dimension resolving `CH.{id}.{costume}...`,
  applied to **dialogue** sprites too (fix the snapback).
- Keep **voiceKey** stable and **path-independent** (never shaken by costume/branching).
- Non-breaking **migration** of existing `???-{id}` and `{id}_{variant}` content; `code=` preserved.
- Stay within language invariants: total, non-Turing, reduces to flat records + CEL; `::set` stays
  narrow.

**Non-goals**
- Collection state (inventory lists/sets/maps) — deferred (YAGNI); current state types
  (`number|bool|enum`) suffice for cast.
- Runtime secrecy of identity (compiled output still carries `assetName`/`CH.{id}` — mystery is a
  presentation choice, not a security property; unchanged from today).

## 3. The model

### 3.1 Identity in the bracket
`:line[bianca]` is always the stable `characterId` (author-facing). It is written from the
character's very first (masked) line — the source is the *author's* view; masking is the *player's*
view, resolved separately. No `-`/`_` overloading.

### 3.2 Character registry (provider)
Global, static identity data, provided as a plugin **provider** (snapshot-first, per plugin-system
proposal §10):

```yaml
# character registry entry (provider)
bianca:
  name: { ko: "비앙카", en: "Bianca" }     # canonical display name per language
  costumes: [default, waitress, casual]      # valid costume domain
  defaultCostume: default
ann:
  name: { ko: "앤", en: "Ann" }
  costumes: [default, child]
  defaultCostume: default
  # per-costume voice bank override (see §3.6) — static
  costumeVoiceBank: { child: ann_child }
```

### 3.3 `cast:` frontmatter (plugin-owned) + new mechanism
Per-episode initial per-character state and static alias labels live in a **plugin-owned
frontmatter key**:

```yaml
---
character: bianca
season: 1
episode: 1
profile: story
cast:
  bianca:
    costume: waitress               # enum ∈ registry.costumes
    sealed: true                   # bool; default false (mask only the rare exception)
    maskAs: "은발의 종업원"          # static label while masked; omitted → "???"
  takeru: { costume: default }       # sealed omitted → false (real name)
---
```

This requires two small spec additions (the user approved language changes):
- **Language `0.0.1` §6.1 amendment:** the fixed core meta keys remain
  (`character/season/episode/mode/pov/title/lang/profile/plugins/uses/state/defs`); additionally,
  **an active plugin MAY contribute meta keys**, validated against that plugin's declared schema. A
  meta key owned by no active plugin is a static error.
- **Plugin-system `0.0.1` addition:** a plugin export **`frontmatter`** — declares meta key names +
  their schema; folded into the capability snapshot; the checker validates the block. `cast` is
  owned by the character/cast plugin.

### 3.4 Mutable state (enum / bool)
The cast plugin declares, per referenced character id (a state template keyed by the character
provider, plugin-system §6.3/§7.4):

- `scene.cast.<id>.costume : enum[<registry.costumes>]` — default `registry.defaultCostume`.
- `scene.cast.<id>.sealed : bool` — default `false`; set `true` (via `cast:` or `::seal`) to mask.
- `maskAs` (static, from `cast:`; not state) — label shown while `sealed == true`; default `"???"`.

Costume is `enum`, `sealed` is `bool` → both satisfy the language's `number|bool|enum` state
types (§9.3). No `unset`, no `null`, no string-typed state anywhere. Scene-scoped (episode
lifetime); because `sealed` defaults `false`, a later episode that omits a character from `cast:`
shows the real name automatically — cross-episode "already known" is free. A rare
multi-episode masked arc binds the mask to a `run.*` fact instead.

### 3.5 Resolution (compile-time)
A `:line[<id>]{code, emotion, variant, as?}` (and `::auto`) resolves three outputs:

1. **Sprite:** `CH.{id}.{scene.cast.<id>.costume}.{emotion}.{variant}` (costume from StageState, §3.7).
2. **Display name:** `line.as` (one-off attr override) ▸ `sealed` ? `maskAs` (default `"???"`) :
   `registry.name[lang]`. (Normative model — `0.0.1.md` §7.2 — resolves `kind: narrator` and
   `kind: player` labels *before* `as`; `as` is highest only for `character` speakers.)
3. **voiceKey:** see §3.6.

`as` is a new **optional core `:line` attribute** (speaker-label override); the cast plugin supplies
the default resolution when `as` is absent.

### 3.6 voiceKey (path-stable)
- **`voiceKey = {voiceBank}-{code}`** where `voiceBank` is a **static, compile-time constant**:
  `registry.costumeVoiceBank[costume]` **only when the costume is statically determinable at that
  line** (frontmatter/dominating static assignment), else `characterId`.
- voiceKey **MUST NOT** be derived from mutable `scene.cast.*.costume` in a way that varies by
  branch/timeline path. Sprites MAY vary per path (not a join key); **voice MUST NOT**.
- **Lint:** if a coded line's `voiceBank` would be path-dependent/ambiguous, it is a static error;
  the author pins it explicitly (or accepts `characterId`).
- Default (no `costumeVoiceBank`) → `voiceKey = {characterId}-{code}`, exactly today's key for the
  hyphen-form migration.

### 3.7 Compiler (StageState)
Per the architecture doc's stateful-resolution model, `StageState.onStage` gains **`costume`** and
**`sealed`** per character. The reducer applies the current costume when emitting **dialogue**
sprite ids (not only `::auto`), fixing the snapback. This is a named injection/resolution concern,
manifest-driven (which directives read/write cast state) but code-executed.

### 3.8 Sugar directives (plugin directives → `::set`)
Thin, pure desugaring (no new semantics); the primary author surface:

- `::seal{character="bianca"}` → `::set{scene.cast.bianca.sealed = true}`
- `::reveal{character="bianca"}` → `::set{scene.cast.bianca.sealed = false}`
- `::wear{character="bianca" costume="casual"}` → `::set{scene.cast.bianca.costume = 'casual'}`

Raw `::set{scene.cast.*}` remains the low-level escape hatch. Conditional/player-driven changes use
`<match>`/`<branch>` arms containing these. `::set` itself stays narrow (single typed CEL assignment)
— its narrowness is the analyzability invariant; expressiveness comes from composing the four layers
(frontmatter init · sugar directives · logic-layer + `::set` · compiler resolution), not from a
fatter `::set`.

## 4. Spec changes required

1. **Language `0.0.1`:** §6.1 plugin-contributed meta keys (§3.3); new optional core `:line`
   attribute `as` (§3.5).
2. **Plugin-system `0.0.1`:** `frontmatter` export mechanism (§3.3); confirm state-template keyed by
   a provider id domain (§3.4).
3. **New normative proposal:** `proposals/character-cast/0.0.1.md` — the cast plugin: registry
   provider schema, `cast:` frontmatter schema, `scene.cast.<id>.*` state shape, `reveal/seal/wear`
   directives, and the sprite/label/voiceKey resolution hooks (named lowering hooks per plugin
   proposal §8.2).
4. **Architecture:** `StageState` carries `costume`/`sealed`; dialogue sprite emission uses it.

## 5. Migration (non-breaking)

| Legacy | New | voiceKey |
|---|---|---|
| `:line[???-bianca]{code=…}` | `:line[bianca]{code=…}` + `cast.bianca.sealed: true` (or `::seal`) | unchanged (`bianca-…`) |
| `:line[???]` (no id) | requires an explicit id — flag for author fix | n/a |
| `:line[ann_child]{code=…}` | `:line[ann]{code=…}` + `cast.ann.costume: child` + `registry.ann.costumeVoiceBank.child = ann_child` | preserved (`ann_child-…`) |
| `:line[nera_formal]{code=…}` | `:line[nera]{code=…}` + `costume: formal` + `costumeVoiceBank.formal = nera_formal` | preserved (`nera_formal-…`) |

- **`code=` values are never changed** (voice joins stable).
- The formatter performs the rewrite and emits deprecation warnings for legacy forms.
- Touch together: `parser.ts`, `generator.ts`, **`minter.ts`** (duplicate hyphen rule), `formatter.ts`,
  their `__tests__`, and the `prompts/` scenario standards.

## 6. Lints / diagnostics

- **Forgot-to-seal:** a character with a later `::reveal` (a `sealed` true→false transition) that
  first appears without being sealed (`sealed: true` / `::seal`) → warn (possible name spoiler).
  Easy to detect since identity is always the bracket.
- **Path-dependent voiceBank:** a coded line whose `voiceBank` is ambiguous across paths → error (§3.6).
- **Unknown costume/character:** `costume` ∉ `registry.costumes`, or `character` ∉ registry → error
  (provider-snapshot validated; stale snapshot → "catalog stale", not a hard error).
- **`<match on="scene.cast.<id>.sealed">`** is a `bool` → finite domain, ordinary exhaustiveness
  (no `unset`/null case).

## 7. Worked example

```lute
---
character: bianca
season: 1
episode: 1
profile: story
cast:
  bianca: { costume: waitress, sealed: true, maskAs: "은발의 종업원" }
---

## Shot 1.
::auto{character="bianca" anchor="center" action="fade-in-up"}
:line[bianca]{code="0010" emotion="content"}: 이쪽으로 오세요, 손님.
/* 화면: "은발의 종업원" · 스프라이트: CH.bianca.waitress.content.0 · voiceKey: bianca-0010 */

## Shot 4.
::reveal{character="bianca"}
:line[bianca]{code="0110" emotion="delighted"}: 저는 비앙카예요.
/* 화면: "비앙카" (registry) · 이후 모든 라인 자동 전환 */

::wear{character="bianca" costume="casual"}
:line[bianca]{code="0120" emotion="shy"}: 편한 옷으로 갈아입었어요.
/* 스프라이트: CH.bianca.casual.shy.0 · voiceKey: bianca-0120 (costume 무관) */
```

## 8. Open / deferred

- Collection state (inventory/sets/maps) + richer `::set` ops — separate future decision (YAGNI).
- Whether reveal needs > 2 states (multi-stage `???`→nickname→real): start with the `sealed`
  bool + per-line `as=` for rare middle stages; promote bool→enum only if a scene needs it.
- Cast plugin id/ownership (`idola.cast` vs generic `lute.cast`) — decide at proposal time; lute
  core stays generic, the VN profile activates it.

## 9. Decisions log (this session)

1. Reveal = **first-class state + per-line override** (override wins) → realized as the
   `seal`/`reveal` pair over a `sealed` **bool** (default `false`; `true` marks the rare masked case — the boolean lives on the rare side).
2. Costume + reveal unified as **per-character scene state**, seeded in frontmatter, changed by
   `::set`/sugar.
3. `cast:` is a **plugin-owned frontmatter** key (user accepted the language/plugin-system change).
4. Mutable state is **enum/bool only** → no `unset`/`null`/string-state; no language op for delete.
5. **`::set` stays narrow**; richness via layer composition.
6. **voiceKey is path-stable**, never shaken by mutable costume; static `voiceBank` override + lint.
