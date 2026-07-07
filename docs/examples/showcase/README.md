# Full-spec Lute showcase

A single self-contained project that exercises **every implemented Lute feature**
end-to-end and checks clean:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p lute-cli
./target/debug/lute check docs/examples/showcase/episode01.lute \
  --project docs/examples/showcase        # exit 0, 0 warnings
```

`lute tag` back-fills stable `code`s into untagged content lines and is idempotent
(smoke-test on a throwaway copy *inside this dir* so `uses:` still resolves —
`uses:`/`extends:` are resolved relative to the scene file, so a bare `/tmp`
copy would report `E-USES-NOT-FOUND`):

```sh
cp docs/examples/showcase/episode01.lute docs/examples/showcase/_t.lute
./target/debug/lute tag docs/examples/showcase/_t.lute            # tags 13 lines
./target/debug/lute check docs/examples/showcase/_t.lute --project docs/examples/showcase   # exit 0
./target/debug/lute tag docs/examples/showcase/_t.lute            # "already tagged"
rm docs/examples/showcase/_t.lute
```

> Schema docs (`schema/*.schema.lute`) and component files (`components/*.component.lute`)
> intentionally fail a *standalone* `lute check` with `E-META-MISSING` — they carry no
> `character`/`season`/`episode` because they are **imported** (validated in import /
> component mode) by the episode, not run as scenes. This matches
> `docs/examples/state.schema.lute` and `extends-base.lute`. The episode check validates
> them transitively.

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | `pluginsDir`/`catalogDir`; `showcase` profile (extends `global`) activates `showcase.pack` |
| `plugins/showcase.pack/plugin.yaml` | plugin manifest — exports **all six** kinds + `options` |
| `plugins/showcase.pack/directives/serve.yaml` | bridge directive `::serve` (providerRef + assetKind + slotId + bridge + state) |
| `plugins/showcase.pack/state/shapes.yaml` | `serveResult` state shape |
| `plugins/showcase.pack/state/templates.yaml` | `serveDefault` state template (`stateTemplates` export) |
| `plugins/showcase.pack/providers/cast.yaml` | `castId` provider registry |
| `plugins/showcase.pack/bridge/serve.yaml` | `serve/play` bridge capability |
| `plugins/showcase.pack/assetkinds/poster.yaml` | `PT.<actor>.<variant>` asset kind (segments) |
| `plugins/showcase.pack/defs/showcase.yaml` | plugin-exported def `@showcaseReady` |
| `catalog/cast.yaml` | pinned `castId` ids (`bianca_star`, `takeru_host`) |
| `schema/base.schema.lute` | base `run`/`user`/`app` state + defs (`helped`, `atLeast(n)`) |
| `schema/game.schema.lute` | `extends: base` — refines `user.level` default, adds `run.chapter` + `veteran` def |
| `components/stinger.component.lute` | reusable content component (dsl §13) — `component:` + `params:` + presentational body, expanded by `::use` |
| `episode01.lute` | the scene wiring it all together |
| `hub-demo.lute` | non-episode companion: a revisit `<hub>` + `<when is>` over hub-recorded enums + `{{…}}` interpolation (checks clean **and** compiles) |
| `when-is-demo.lute` | non-episode companion: `<when is>` literal arms (incl. `\|`-alternation) over a plain scene-local enum |

## Plugin export kinds shipped (all six)

`directives/` · `state/` (shapes + templates) · `providers/` · `bridge/` · `assetkinds/` · `defs/`

## Feature → location map

### Frontmatter (`episode01.lute`)
| Feature | Line |
|---|---|
| `mode` / `title` / `pov` / `luteVersion` | 2, 6, 7, 8 |
| `character` / `season` / `episode` | 3–5 |
| `profile` (root capability selector) | 10 |
| `plugins` (scene-local activation + options) | 13–16 |
| `uses:` (import child schema) | 19 |
| `components:` (import content components, dsl §13) | 22 |
| inline `state:` (scene tier) | 24–26 |
| inline `defs:` (`@fond`) | 28–29 |
| `extends:` (composition) | `schema/game.schema.lute:6` |

### State tiers (all four) + writes + policy
| Feature | Location |
|---|---|
| `scene.*` decl + default | `episode01.lute:25–26` |
| `run.*` decl + default | `schema/base.schema.lute:6–8`, `schema/game.schema.lute:9` |
| `user.*` decl + default (base→child override) | `schema/base.schema.lute:9` → `schema/game.schema.lute:8` |
| `app.*` decl + default | `schema/base.schema.lute:10–11` |
| `::set` pure `=` write | `episode01.lute:77` |
| `::set` compound op (`+=`) | `episode01.lute:100, 104, 131` |
| write policy respected (no `app.*` write) | `app.rating` (188) + `app.lang` (230) only read — no `::set` targets `app.*` |
| definite assignment (defaulted / bridge-dominated / guarded reads) | throughout; bridge write @89 dominates read @97 |

### Expressions
| Feature | Location |
|---|---|
| inline `@ref` | `@fond` — `episode01.lute:146` |
| plugin-exported `@ref` | `@showcaseReady` — `episode01.lute:125` |
| parameterized `@name(args)` | `@atLeast(3)` — `episode01.lute:203`; `@atLeast(1)` — `129` |
| `$` match subject | `episode01.lute:98, 102, 149, 164, 167, 173, 189, 231` |
| child-schema def via extends | `@veteran` — `episode01.lute:206` |

### Logic
| Feature | Location |
|---|---|
| `<branch>` + `<choice>` | `episode01.lute:121–133` |
| `<choice when=…>` guards | lines 125, 129 |
| `persist="run"` sugar — bool (default value) | line 125 (`into="run.metHelpfully"`) |
| `persist="run"` sugar — enum (explicit `value`) | line 129 (`into="run.sofaOutcome" value="warm"`) |
| `<match>` / `<when>` / `<otherwise>` | 97–109, 145–155, 172–179, 188–195, 202–212, 230–237 |
| exhaustive match, no `<otherwise>` (bool domain) | 163–170 |
| maybe-unset subject covered by `<otherwise>` | 172–179 (`run.sofaOutcome`) |
| age-gated `app.rating` match | 188–195 |
| maybe-unset `app.lang` match (enum, `<otherwise>`) | 230–237 |
| choice-key read (`scene.choices.approach`) | 145 |

### Content & directives
| Feature | Location |
|---|---|
| `:narrator` | `episode01.lute:48, 190, 193` |
| `:speaker` w/ attrs (`code`/`emotion`/`variant`) | 49, 99, 103, … |
| `:speaker{delivery="thought"}` monologue | 79, 113, 147, … |
| core staging directives (`::bg` `::music` `::sfx` `::auto` `::camera` `::cut` `::vfx`) | 43–46, 68–73, 136, 222–223 |
| plugin directive `::serve` | 89 |
| plugin attr `providerRef` id (`performer`) | 89 → `catalog/cast.yaml` |
| plugin attr `assetKind` id (decomposed `PT.bianca_star.0`) | 89 → `assetkinds/poster.yaml` |

### Reusable content components (dsl §13)
| Feature | Location |
|---|---|
| `components:` import (DAG, canonicalized/deduped like `uses:`) | `episode01.lute:22` |
| component file (`component:` + `params:` + presentational body) | `components/stinger.component.lute:7–9` |
| `::use{ component=… <arg>=… }` invocation | `episode01.lute:220` |
| `@param` ref (`@cue`) in body attr positions | `components/stinger.component.lute:17, 18` |

### Timeline (`episode01.lute:60–75`)
| Feature | Line |
|---|---|
| `<timeline duration=…>` | 60 |
| `subject` track (camera) | 67 |
| `channel` track (fg) | 71 |
| TWO `property` tracks on one subject (`bianca.pos`, `bianca.opacity`) | 61, 64 |
| clips with absolute `at` | 69, 72, 73 |

### Composition
| Feature | Location |
|---|---|
| base schema | `schema/base.schema.lute` |
| child `extends:` base + refines a default | `schema/game.schema.lute:6, 8` |
| episode `uses:` the child | `episode01.lute:19` |

### `<hub>` + `<when is>` + `{{…}}` (`hub-demo.lute`)
| Feature | Line |
|---|---|
| `{{…}}` content interpolation (§7.6) — `{{userName}}` / `{{run.affection}}` | 40–41 |
| `<hub>` revisit menu (§7.3.2) — `once` / `when`-guarded / `exit` choices | 54–65 |
| `<when is>` literal arms over hub-recorded enum `scene.choices.*` ∪ `unset` (§7.3.1) | 74–87 |
| `<when is>` bool arms over hub-recorded `scene.visited.*.*` (§7.3.1) | 94–101 |

### `<when is>` over a plain scene enum (`when-is-demo.lute`)
| Feature | Line |
|---|---|
| scene-local ENUM decl + default (definitely assigned → no `unset` case) | 19 |
| `<when is>` literal-pattern arms over a PLAIN scene enum `scene.mood` (§7.3.1) | 47–57 |
| singleton literal arms (`is="calm"`, `is="tense"`) | 48, 51 |
| `is="a\|b"` alternation arm (`is="joyful\|playful"`, §7.3.1) | 54 |
| exhaustive `is` coverage, NO `<otherwise>` (§11.2) | 47–57 |
