# Full-spec Lute showcase

A single self-contained project that exercises **every implemented Lute feature**
end-to-end and checks clean:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p lute-cli
./target/debug/lute check docs/examples/showcase/episode01.lute \
  --project docs/examples/showcase        # exit 0, 0 warnings
```

`lute tag` back-fills stable `code`s into untagged `:line`s and is idempotent
(smoke-test on a throwaway copy *inside this dir* so `uses:` still resolves —
`uses:`/`extends:` are resolved relative to the scene file, so a bare `/tmp`
copy would report `E-USES-NOT-FOUND`):

```sh
cp docs/examples/showcase/episode01.lute docs/examples/showcase/_t.lute
./target/debug/lute tag docs/examples/showcase/_t.lute            # tags 11 lines
./target/debug/lute check docs/examples/showcase/_t.lute --project docs/examples/showcase   # exit 0
./target/debug/lute tag docs/examples/showcase/_t.lute            # "already tagged"
rm docs/examples/showcase/_t.lute
```

> Schema docs (`schema/*.schema.lute`) intentionally fail a *standalone* `lute check`
> with `E-META-MISSING` — they carry no `character`/`season`/`episode` because they
> are **imported** (validated in import mode) by the episode, not run as scenes. This
> matches `docs/examples/state.schema.lute` and `extends-base.lute`. The episode check
> validates them transitively.

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
| `episode01.lute` | the scene wiring it all together |

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
| inline `state:` (scene tier) | 21–23 |
| inline `defs:` (`@fond`) | 25–26 |
| `extends:` (composition) | `schema/game.schema.lute:6` |

### State tiers (all four) + writes + policy
| Feature | Location |
|---|---|
| `scene.*` decl + default | `episode01.lute:22–23` |
| `run.*` decl + default | `schema/base.schema.lute:6–8`, `schema/game.schema.lute:9` |
| `user.*` decl + default (base→child override) | `schema/base.schema.lute:9` → `schema/game.schema.lute:8` |
| `app.*` decl + default | `schema/base.schema.lute:10–11` |
| `::set` pure `=` write | `episode01.lute:74` |
| `::set` compound op (`+=`) | `episode01.lute:97, 101, 128` |
| write policy respected (no `app.*` write) | `app.rating` (185) + `app.lang` (219) only read — no `::set` targets `app.*` |
| definite assignment (defaulted / bridge-dominated / guarded reads) | throughout; bridge write @86 dominates read @94 |

### Expressions
| Feature | Location |
|---|---|
| inline `@ref` | `@fond` — `episode01.lute:143` |
| plugin-exported `@ref` | `@showcaseReady` — `episode01.lute:122` |
| parameterized `@name(args)` | `@atLeast(3)` — `episode01.lute:200`; `@atLeast(1)` — `126` |
| `$` match subject | `episode01.lute:95, 99, 146, 161, 164, 170, 186, 220` |
| child-schema def via extends | `@veteran` — `episode01.lute:203` |

### Logic
| Feature | Location |
|---|---|
| `<branch>` + `<choice>` | `episode01.lute:118–130` |
| `<choice when=…>` guards | lines 122, 126 |
| `persist="run"` sugar — bool (default value) | line 122 (`as="run.metHelpfully"`) |
| `persist="run"` sugar — enum (explicit `value`) | line 126 (`as="run.sofaOutcome" value="warm"`) |
| `<match>` / `<when>` / `<otherwise>` | 94–106, 142–152, 169–176, 185–192, 199–209, 219–226 |
| exhaustive match, no `<otherwise>` (bool domain) | 160–167 |
| maybe-unset subject covered by `<otherwise>` | 169–176 (`run.sofaOutcome`) |
| age-gated `app.rating` match | 185–192 |
| maybe-unset `app.lang` match (enum, `<otherwise>`) | 219–226 |
| choice-key read (`scene.choices.approach`) | 142 |

### Content & directives
| Feature | Location |
|---|---|
| `:line[narrator]` | `episode01.lute:45, 187, 190` |
| `:line[speaker]` w/ attrs (`code`/`emotion`/`variant`) | 46, 96, 100, … |
| `:line` monologue (`delivery="thought"`) | 76, 110, 144, … |
| core staging directives (`::bg` `::music` `::sfx` `::auto` `::camera` `::cut` `::vfx`) | 40–43, 65–70, 133, 211–212 |
| plugin directive `::serve` | 86 |
| plugin attr `providerRef` id (`performer`) | 86 → `catalog/cast.yaml` |
| plugin attr `assetKind` id (decomposed `PT.bianca_star.0`) | 86 → `assetkinds/poster.yaml` |

### Timeline (`episode01.lute:57–72`)
| Feature | Line |
|---|---|
| `<timeline duration=…>` | 57 |
| `subject` track (camera) | 64 |
| `channel` track (fg) | 68 |
| TWO `property` tracks on one subject (`bianca.pos`, `bianca.opacity`) | 58, 61 |
| clips with absolute `at` | 66, 69, 70 |

### Composition
| Feature | Location |
|---|---|
| base schema | `schema/base.schema.lute` |
| child `extends:` base + refines a default | `schema/game.schema.lute:6, 8` |
| episode `uses:` the child | `episode01.lute:19` |
