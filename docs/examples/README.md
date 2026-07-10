# Lute examples — layout & naming

| Suffix / file | Role | Has `kind:`? | How it's used |
|---|---|---|---|
| `*.lute` (scene) | a scene document | `kind: scene` | run/checked as a root |
| `*.lute` (quest) | a quest document | `kind: quest` | run/checked as a root |
| `*.schema.lute` | state/defs schema (no body) | no | imported via `uses:` / `extends:` |
| `*.component.lute` | reusable content component | no | imported via `components:` + `::use` |
| `lute.project.yaml` | project root: profiles + plugin/catalog dirs | — | discovered by the CLI/LSP up the tree |
| `plugins/<id>/` | a capability plugin package (typed YAML) | — | activated by a profile |
| `catalog/*.yaml` | pinned provider id snapshots | — | resolved by `providerRef` |

- Open a **root** (`kind:` scene/quest) to check a project; schema/component files are
  validated transitively and, opened standalone, are recognized by frontmatter shape (no
  false `E-KIND-MISSING`).
- Examples that use plugin directives resolve their plugins via the nearest `lute.project.yaml`.
