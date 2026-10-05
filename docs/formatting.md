# Formatting rules (0.36.1)

`lute fmt <path>…` canonicalizes source without changing its meaning. It walks
selected inputs in sorted path order and is deterministic, so repeated runs are
idempotent. `--check` performs the same comparison without writing: it exits 0
when canonical, 1 when a file would change, and 2 for I/O, UTF-8, or parse
failures.

Formatting applies to `.lute` documents and the project, schema, and configured
plugin YAML files that Lute owns. It normalizes Lute trivia: indentation,
spacing, line endings, delimiters, and canonical source layout. It preserves
comments, authored strings, line IDs, ordering with semantic meaning, and all
content values.

YAML is opaque: the formatter does not parse and reserialize arbitrary YAML or
rewrite YAML values. Only the selected Lute-owned YAML inputs participate in
selection; their values remain untouched. Symlinks are skipped.

Formatting is not validation, compilation, or migration. Use `lute check` or
`check-project` for diagnostics, and review `lute diff` when comparing revisions.
