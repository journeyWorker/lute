# Full-Spec Showcase Example — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Goal:** Build a self-contained `docs/examples/showcase/` project — a plugin + project + schema docs + scene(s) — that exercises EVERY implemented Lute feature end-to-end, and prove it all checks clean. This is the acceptance showcase for the whole toolchain.

**Architecture:** One project dir with a capability plugin (all export kinds INCLUDING `defs`), a base + extending schema (`extends:`), and an episode scene that uses `uses:`/inline defs/parameterized `@name(args)`/state (all tiers)/`<branch>`+`<choice persist>`/`<match>`/`<timeline>` with property tracks/plugin directives with assetKind + providerRef ids. Every scene MUST check exit 0 (genuinely).

**Tech Stack:** authored `.lute` + `.yaml` fixtures; verified with `./target/debug/lute check`.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- No source-code changes — this is fixtures + docs only. If the checker rejects something that SHOULD be valid, STOP and report (do not hack the checker); otherwise fix the fixture to be genuinely valid.
- Every showcase scene MUST `lute check` exit 0 (no masking). Study existing fixtures for correct syntax: `docs/examples/{bianca-s01ep02.lute, carry-ep.lute, state.schema.lute, extends-*.lute, choice-persist.lute, property-tracks.lute, param-def.lute, plugin-def.lute, idola-project/*}`.

## Feature coverage checklist (the showcase MUST exercise ALL)
- Frontmatter: `character`, `season`, `episode`, `profile`, `plugins`, inline `defs:`, inline `state:` (scene tier), `uses:`, `extends:`.
- Project + plugin: `lute.project.yaml` with profiles activating a plugin; plugin `plugin.yaml` exporting `directives/`, `state/` (shapes + templates), `providers/`, `bridge/`, `assetkinds/`, AND `defs/` (plugin def-export).
- State: `scene.*`, `run.*`, `user.*`, `app.*` declarations + defaults; definite assignment; `::set` writes (`=` and a compound op); write policy respected (no `app.*` write).
- Expressions: inline `@ref`, plugin-exported `@ref`, parameterized `@name(args)` def call with correct arity + arg types, `$` match subject.
- Logic: `<branch>` + `<choice>` with `when` guards AND `persist="run" as="run.<path>" [value=…]` sugar; `<match on=…>` with `<when>`/`<otherwise>` covering the domain.
- Content: `:line[speaker]` (incl. narrator) + directives with attrs.
- Directives: a core staging directive AND a plugin directive; a plugin directive attr using a `providerRef` id and an `assetKind` id (decomposed segments).
- Timeline: `<timeline duration=…>` with a `subject` track, a `channel` track, and TWO `property` tracks on one subject (split-subject), clips with `at`.
- Composition: a base schema + a child schema that `extends:` it and refines a default; the episode `uses:` the child.

---

## Task 1: Build the showcase project + scenes

**Files (create under `docs/examples/showcase/`):**
- `lute.project.yaml` — `pluginsDir: plugins/`, a default profile activating `showcase.pack`.
- `plugins/showcase.pack/plugin.yaml` — exports: directives, state, providers, bridge, assetkinds, defs.
- `plugins/showcase.pack/directives/*.yaml` — ≥1 bridge/staging directive (with attrs: a `providerRef` and an `assetKind` typed attr), following `idola-project/plugins/idola.minigame/directives/minigame.yaml`.
- `plugins/showcase.pack/state/shapes.yaml` (+ `templates.yaml` if used) — a state shape the directive declares.
- `plugins/showcase.pack/providers/*.yaml` — an id registry for the providerRef.
- `plugins/showcase.pack/bridge/*.yaml` — a bridge capability.
- `plugins/showcase.pack/assetkinds/*.yaml` — an asset kind (segments), following `idola.minigame/assetkinds/art.yaml`.
- `plugins/showcase.pack/defs/*.yaml` — a plugin-exported def (list form `- { name, type, cel }`).
- `catalog/*.yaml` — the provider catalog (if the project resolves providers from `catalog/`, per idola-project) so providerRef ids resolve.
- `schema/base.schema.lute` — base `run.*`/`user.*` state + shared defs.
- `schema/game.schema.lute` — `extends: base.schema.lute`; refines a default + adds state/defs.
- `episode01.lute` — the main scene: `uses: schema/game.schema.lute`, exercises the full checklist above.
- `README.md` — a short feature→file/line map (what each part demonstrates).

- [ ] **Step 1:** Author the plugin + project + catalog. Verify the plugin/project resolve: `export PATH="$HOME/.cargo/bin:$PATH" && cargo build -p lute-cli` then a trivial scene check against `--project docs/examples/showcase` to confirm the snapshot assembles with no `E-PLUGIN-*` errors. Fix any manifest issues.
- [ ] **Step 2:** Author `schema/base.schema.lute` + `schema/game.schema.lute` (extends + refine). 
- [ ] **Step 3:** Author `episode01.lute` covering the full checklist. Iterate: `./target/debug/lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase; echo exit=$?` until exit 0 AND it genuinely uses every checklist item. If a genuinely-valid construct is rejected, STOP + report (possible real bug); else fix the fixture.
- [ ] **Step 4:** Confirm each schema doc also checks (schema-mode) without spurious errors when imported (the episode check covers this transitively).
- [ ] **Step 5:** `lute tag` smoke on a TEMP COPY of episode01 (never the committed file): `cp docs/examples/showcase/episode01.lute /tmp/sc.lute && ./target/debug/lute tag /tmp/sc.lute && ./target/debug/lute check /tmp/sc.lute --project docs/examples/showcase; echo exit=$?` → tags added, still exit 0, idempotent on rerun.
- [ ] **Step 6:** Write `README.md` mapping each feature to where it appears.
- [ ] **Step 7:** Commit:
```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add docs/examples/showcase
git commit -m "docs(examples): full-spec showcase project (plugin+extends+match+branch/persist+timeline/property+assetKind+@name(args))"
```

## Verification (controller, after review)
```
./target/debug/lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase   # exit 0
cargo test --workspace   # unaffected (fixtures only)
```
Plus the controller re-runs every prior acceptance fixture to confirm no regression.

## Self-Review
- Every checklist feature appears in the showcase and the scene checks exit 0 (not by masking).
- Plugin exports ALL kinds incl. `defs`; the episode uses a plugin def via `@ref` and a plugin directive with providerRef + assetKind ids.
- `extends` composition + `uses` import both exercised; `<choice persist>` + property tracks + `@name(args)` all present.
- README maps features → locations.
