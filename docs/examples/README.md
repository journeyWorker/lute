# Lute examples — layout & naming

| Suffix / file | Role | Has `kind:`? | How it's used |
|---|---|---|---|
| `*.lute` (scene) | a scene document | `kind: scene` | run/checked as a root |
| `*.lute` (quest) | a quest document | `kind: quest` | run/checked as a root |
| `*.schema.yaml` | state/defs schema (no body) | no | imported via `uses:` / `extends:` |
| `*.component.lute` | reusable content component | no | imported via `components:` + `::use` |
| `lute.project.yaml` | project root: profiles + plugin/catalog dirs | — | discovered by the CLI/LSP up the tree |
| `plugins/<id>/` | a capability plugin package (typed YAML) | — | activated by a profile |
| `catalog/*.yaml` | pinned provider id snapshots | — | resolved by `providerRef` |

- Open a **root** (`kind:` scene/quest) to check a project. `*.component.lute` fragments,
  opened standalone, are recognized by frontmatter shape (no false `E-KIND-MISSING`);
  `*.schema.yaml` declarations are validated transitively via `uses:`/`extends:`, and — when
  they live under a project's `schema/`/`catalog/` dir — get direct semantic lint in-editor
  (LSP declaration claim) plus structural lint from the shipped JSON Schema (`yaml.schemas`).
- Examples that use plugin directives resolve their plugins via the nearest `lute.project.yaml`.
- [`connect-scenes/`](connect-scenes/) is the project the website's *Connect scenes into a story* page builds: scenes chained with one shape-only occasion (`on: chapter`), `after:`, descending `priority:` and `when:` endings, played by `plays/*.play.yaml` and pinned by `tests/`. [`episodes/`](episodes/) is the two-scene project of *Write your first scene*.
- Worked-example spot picks: [`quest-grove.lute`](quest-grove.lute) (a lone quest with authored `fail=` and derived completion), [`quest-subquest.lute`](quest-subquest.lute) (a parent that names required + optional children via `<objective quest="…"/>`, spec: [`docs/superpowers/specs/2026-08-31-lute-subquest-design.md`](../superpowers/specs/2026-08-31-lute-subquest-design.md)), and [`capability-permissions/`](capability-permissions/) (absent versus explicit-empty permission fields, an authored reward accepted normally but denied by a trusted host profile, and a forbidden streamed write rejected before IR emission).

[`quayside/`](quayside/) is the complete town-game tutorial project:
it combines a named clock, gated occasions, static lore barks, play refusals,
named ending bindings, a player-web host recipe, a second play-through
(`user.*`, `prev.run`, `plays/second-run.play.yaml`) and a Korean translation
(`locales/ko.json` from `lute loc export`, `locales/bundle.json` from `lute loc import`).

[`flow-and-text/`](flow-and-text/) is the 0.51 "flow and text" project: a bounded `::jump{max}` retry loop over a bridge result and a nested loop, a directive and a reward kind whose `effects.raises` reach a `raisedBy: declared` occasion, a `::banner` with `type: text` attributes, `code=` and a Korean locale bundle (`loc/ko.json`), and a `target: pair` lint, pinned by `tests/` and `plays/`. It needs the 0.51 toolchain.

## Games

[`games/`](games/) holds fourteen complete games written against Lute 0.21–0.28 as dogfood
projects — mysteries, a roguelike, a gacha, live-ops, a visual novel, an Ink port and more. Each is
its own project (`games/<slug>/lute.project.yaml`) with a README naming the features it exercises,
and CI checks each with `lute check --deny-warnings --project <game>` and runs its tests and plays. Start at
the [index](games/README.md).
