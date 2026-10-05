---
title: Formatting rules
description: What lute fmt changes and preserves.
---

# Formatting rules (0.36.2)

`lute fmt <path>…` canonicalizes source without changing its meaning. It is
deterministic and idempotent. `--check` writes nothing: exit 0 means canonical,
1 means a selected file would change, and 2 means I/O, UTF-8, or parse failure.

Formatting applies to `.lute` documents and Lute-owned project, schema, and
configured plugin YAML inputs. It normalizes trivia such as indentation,
spacing, line endings, delimiters, and source layout while preserving comments,
authored strings, line IDs, semantic ordering, and values.

YAML values are opaque: the formatter does not reserialize arbitrary YAML or
rewrite values. Symlinks are skipped. Formatting is not validation or
compilation; use `lute check` or `check-project` for diagnostics.
