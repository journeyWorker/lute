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
./target/debug/lute tag docs/examples/showcase/_t.lute            # tags 14 lines
./target/debug/lute check docs/examples/showcase/_t.lute --project docs/examples/showcase   # exit 0
./target/debug/lute tag docs/examples/showcase/_t.lute            # "already tagged"
rm docs/examples/showcase/_t.lute
```

While a scene is still being drafted, `lute tag --force` renumbers EVERY line's
`code` in clean document order (0010/0020/… per speaker per identity scope),
rewriting existing codes — useful after insertions/deletions leave the sequence
gappy. Once codes are published identity (exported for localization or voice),
declare `codesLocked: true` in frontmatter and `--force` refuses to renumber
(plain `lute tag` back-fill stays available — new lines never break the
`lineId` join).

> Schema declarations (`schema/*.schema.yaml`) and component files (`components/*.component.lute`)
> carry no `character`/`season`/`episode` because they are **imported** (validated in
> import / component mode) by the episode, not run as scenes — like
> `docs/examples/state.schema.yaml` and `base.schema.yaml`. Since 0.2.1 fragment-shape
> inference (dsl §D6c), a `component:`-shaped `.lute` fragment is recognized by shape and no
> longer raises a false `E-KIND-MISSING`/`E-META-MISSING` standalone; since 0.3.0 B2/B4 a
> `.schema.yaml` declaration is a pure map (no `---` envelope, no body) resolved by `uses:`/
> `extends:` import and — under `schema:`/`catalog:` — claimed by the LSP directly; the
> episode check still validates each import transitively either way.

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | `pluginsDir`/`catalogDir`; `showcase` profile (extends `global`) activates `showcase.pack` |
| `plugins/showcase.pack/plugin.yaml` | plugin manifest — exports **all seven** kinds + `options` |
| `plugins/showcase.pack/directives/serve.yaml` | bridge directive `::serve` (providerRef + assetKind + slotId + bridge + state) |
| `plugins/showcase.pack/state/shapes.yaml` | `serveResult` state shape |
| `plugins/showcase.pack/state/templates.yaml` | `serveDefault` state template (`stateTemplates` export) |
| `plugins/showcase.pack/providers/cast.yaml` | `castId` provider registry |
| `plugins/showcase.pack/bridge/serve.yaml` | `serve/play` bridge capability |
| `plugins/showcase.pack/assetkinds/poster.yaml` | `PT.<actor>.<variant>` asset kind (segments) |
| `plugins/showcase.pack/defs/showcase.yaml` | plugin-exported def `@showcaseReady` |
| `plugins/showcase.pack/enums/vocabulary.yaml` | plugin-shipped enum vocabulary members (`enums` export) |
| `catalog/cast.yaml` | pinned `castId` ids (`marina_star`, `kenshi_host`) |
| `schema/base.schema.yaml` | base `run`/`user`/`app` state + defs (`helped`, `atLeast(n)`) |
| `schema/game.schema.yaml` | `extends: base` — refines `user.level` default, adds `run.chapter` + `veteran` def |
| `components/stinger.component.lute` | reusable content component (dsl §13) — `component:` + `params:` + presentational body, expanded by `::use` |
| `episode01.lute` | the scene wiring it all together |
| `hub-demo.lute` | non-episode companion: a revisit `<hub>` + `<when is>` over hub-recorded enums + `{{…}}` interpolation (checks clean **and** compiles) |
| `when-is-demo.lute` | non-episode companion: `<when is>` literal arms (incl. `\|`-alternation) over a plain scene-local enum |

## Plugin export kinds shipped (all seven)

`directives/` · `state/` (shapes + templates) · `providers/` · `bridge/` · `assetkinds/` · `defs/` · `enums/`

## Feature → location map

### Frontmatter (`episode01.lute`)
| Feature | Line |
|---|---|
| `kind` / `mode` | 2, 3 |
| `character` / `season` / `episode` | 4–6 |
| `title` / `pov` / `luteVersion` | 7, 8, 9 |
| `profile` (root capability selector) | 11 |
| `plugins` (scene-local activation + options) | 14–17 |
| `uses:` (import child schema) | 20 |
| `components:` (import content components, dsl §13) | 23 |
| inline `state:` (scene tier) | 25–27 |
| inline `defs:` (`@fond`) | 29–30 |
| `extends:` (composition) | `schema/game.schema.yaml:5` |

### State tiers (all four) + writes + policy
| Feature | Location |
|---|---|
| `scene.*` decl + default | `episode01.lute:26–27` |
| `run.*` decl + default | `schema/base.schema.yaml:5–7`, `schema/game.schema.yaml:8` |
| `user.*` decl + default (base→child override) | `schema/base.schema.yaml:8` → `schema/game.schema.yaml:7` |
| `app.*` decl + default | `schema/base.schema.yaml:9–10` |
| `::set` pure `=` write | `episode01.lute:74` |
| `::set` compound op (`+=`) | `episode01.lute:95, 99, 121, 125` |
| write policy respected (no `app.*` write) | `app.rating` (179) + `app.lang` (218) only read — no `::set` targets `app.*` |
| definite assignment (defaulted / bridge-dominated / guarded reads) | throughout; bridge write @85 dominates read @92 |

### Expressions
| Feature | Location |
|---|---|
| inline `@ref` | `@fond` — `episode01.lute:139` |
| plugin-exported `@ref` | `@showcaseReady` — `episode01.lute:119` |
| parameterized `@name(args)` | `@atLeast(3)` — `episode01.lute:193`; `@atLeast(1)` — `123` |
| `<match subject=…>` | `episode01.lute:92, 138, 155, 164, 179, 192, 218` |
| child-schema def via extends | `@veteran` — `episode01.lute:196` |

### Logic
| Feature | Location |
|---|---|
| `##` sections | `episode01.lute:39, 50, 78, 106, 132, 173` |
| `<branch>` + `<choice>` | `episode01.lute:115–127` |
| `<choice when=…>` guards | lines 119, 123 |
| `into=` run-record sugar — bool (default value) | line 119 (`into="run.metHelpfully"`) |
| `into=` run-record sugar — enum (explicit `value`) | line 123 (`into="run.sofaOutcome" value="warm"`) |
| `<match>` / `<when>` / `<otherwise>` | 92–104, 138–148, 164–171, 179–186, 192–202, 218–225 |
| `<when is="a\|b">` alternation arm | 97 (`is="silver\|bronze"`) |
| exhaustive match, no `<otherwise>` (bool domain) | 155–162 |
| maybe-unset subject covered by `<otherwise>` | 164–171 (`run.sofaOutcome`) |
| age-gated `app.rating` match | 179–186 |
| maybe-unset `app.lang` match (enum, `<otherwise>`) | 218–225 |
| choice-key read (`scene.choices.approach`) | 138 |

### Content & directives
| Feature | Location |
|---|---|
| `@narrator` | `episode01.lute:46, 181, 184, 220, 223` |
| `@speaker{…}` w/ attrs (`code`/`emotion`/`variant`) | 47, 94, 98, … |
| `@speaker{mono}` interior monologue | 76, 108, 140, … |
| core staging directives (`::bg` `::music` `::sfx` `::actor` `::camera` `::cg` `::vfx`) | 41–44, 59–70, 130, 211–212 |
| plugin directive `::serve` | 85 |
| plugin attr `providerRef` id (`performer`) | 85 → `catalog/cast.yaml` |
| plugin attr `assetKind` id (decomposed `PT.marina_star.0`) | 85 → `assetkinds/poster.yaml` |

### Reusable content components (dsl §13)
| Feature | Location |
|---|---|
| `components:` import (DAG, canonicalized/deduped like `uses:`) | `episode01.lute:23` |
| component file (`component:` + `params:` + presentational body) | `components/stinger.component.lute:12–14`, body 17–30 |
| `::use{ component=… <arg>=… }` invocation | `episode01.lute:209` |
| `@param` ref (`@cue`) in body attr positions | `components/stinger.component.lute:28, 29` |

### Timeline (`episode01.lute:57–72`)
| Feature | Line |
|---|---|
| `<timeline duration=…>` | 57 |
| `subject` track (camera) | 64 |
| `channel` track (fg) | 68 |
| TWO `property` tracks on one subject (`marina.pos`, `marina.opacity`) | 58, 61 |
| clips with absolute `at` | 66, 69, 70 |

### Composition
| Feature | Location |
|---|---|
| base schema | `schema/base.schema.yaml` |
| child `extends:` base + refines a default | `schema/game.schema.yaml:5, 7` |
| episode `uses:` the child | `episode01.lute:20` |

### `<hub>` + `<when is>` + `{{…}}` (`hub-demo.lute`)
| Feature | Line |
|---|---|
| `{{…}}` content interpolation (§7.6) — `{{userName}}` / `{{run.affection}}` | 44–45 |
| `<hub>` revisit menu (§7.3.2) — `once` / `when`-guarded / `exit` choices | 57–68 |
| `<when is>` literal arms over hub-recorded enum `scene.choices.*` ∪ `unset` (§7.3.1) | 76–89 |
| `<when is>` bool arms over hub-recorded `scene.visited.*.*` (§7.3.1) | 95–102 |

### `<when is>` over a plain scene enum (`when-is-demo.lute`)
| Feature | Line |
|---|---|
| scene-local ENUM decl + default (definitely assigned → no `unset` case) | 26 |
| `<when is>` literal-pattern arms over a PLAIN scene enum `scene.mood` (§7.3.1) | 51–61 |
| singleton literal arms (`is="calm"`, `is="tense"`) | 52, 55 |
| `is="a\|b"` alternation arm (`is="joyful\|playful"`, §7.3.1) | 58 |
| exhaustive `is` coverage, NO `<otherwise>` (§11.2) | 51–61 |
