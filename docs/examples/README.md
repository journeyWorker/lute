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
