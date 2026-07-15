---
title: Providers and catalog
description: Snapshot-first provider resolution — why the checker never depends on a live catalog, the --providers flag, catalog refresh re-stamping, and capabilityVersion.
---

Plugins add **providers** — id registries that supply the concrete ids a directive's attrs resolve against (background locations, music tracks, character ids, asset ids). Compiler and checker correctness must **never depend on a live or remote catalog**, so providers resolve against a pinned **snapshot artifact** on disk. The parser never calls providers; only the checker does.

## Snapshot-first resolution

Because resolution is against pinned snapshots, the compiler fails cleanly if required data is missing rather than blocking on the network, and the LSP keeps a stale snapshot and emits a *catalog-stale* diagnostic when offline — never false *unknown-id* errors. This keeps a build reproducible: the same pinned inputs always resolve the same ids.

## `--providers`

`check`, `check-project`, `compile`, `context`, `trace`, and `scenario` all accept `--providers <DIR>`, a directory of pinned snapshot files whose ids the document is resolved against.

```console
$ lute check scene.lute --providers ./catalog
```

Precedence: an explicit `--providers <DIR>` wins; otherwise, under `--project`, the project's own pinned catalog is auto-discovered through the same shared helper the LSP uses, so the two surfaces resolve the same ids for the same project; with neither, the provider set is empty.

## `catalog refresh` and `capabilityVersion`

Activation resolves a project's installed plugins plus its selected profile into one immutable **capability snapshot**, stamped with a `capabilityVersion`. Provider snapshots are pinned against that version. When the resolved capability version moves, the pinned snapshots need re-stamping:

```console
$ lute catalog refresh ./catalog --project ./my-game
```

`catalog refresh <dir>` re-stamps every pinned snapshot in `<dir>` against the current `capabilityVersion` and clears its `stale` flag, rewriting each file in the flat on-disk format the loader reads. Under `--project`, it stamps the resolved multi-plugin `capabilityVersion`; omit it for the core baseline. Correctness never depends on a live catalog — refresh only canonicalizes and re-stamps artifacts already pinned, so `refresh` then `load` round-trips exactly. An explicit `catalog refresh` precedes a build. Exit **0** on success, **2** on I/O.
