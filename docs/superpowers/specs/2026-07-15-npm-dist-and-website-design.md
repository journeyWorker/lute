# npm Distribution (`lutecli`) + Website — Design

Date: 2026-07-15
Status: approved (user, this session)
Superseded (2026-07-16): the npm package name was changed from the unscoped `lutecli` to the scoped `@lute-lang/lute` (platform packages `@lute-lang/lute-core-<platform>`). The installed bin stays `lute`. This document records the original naming decision; live manifests/workflows/docs reflect the scoped name.
Reference sibling: `~/Workspace/canon` (bun-wrapped npm distribution + Astro Starlight website)

## Goal

1. Distribute the `lute` CLI as `bunx lutecli` (installed bin: `lute`) via npm, wrapping prebuilt
   Rust native binaries — the same pattern canon ships with (`canoncli`, itself adapted from
   tokscale).
2. Build the Lute website: an Astro Starlight docs site with a custom landing page, en + ko
   locales, deployed to Vercel. Theme: hybrid — lute-instrument identity (warm amber/gold on
   midnight navy, serif headings) with VN scene/dialogue demos as content.

## Decisions (locked)

| Decision | Value |
|---|---|
| npm package name | `lutecli` (`lute` and `lute-cli` are taken on npm; `lutecli` mirrors `canoncli`) |
| Installed bin | `lute` (matches `crates/lute-cli`'s `[[bin]] name = "lute"` — no Rust change) |
| Platform packages | `lutecli-core-darwin-arm64`, `lutecli-core-linux-x64` (unscoped, `os`/`cpu` gated) |
| Site framework | Astro Starlight + custom splash landing |
| Locales | en (root) + ko (core pages first) |
| Theme | Hybrid: bard/instrument identity + VN dialogue demo accents |
| Fonts | Fraunces (headings) / system (body) / JetBrains Mono (code) |
| Palette | Midnight navy background, amber/gold accents, subtle violet glow on VN demo boxes |
| Deploy | Vercel (website), npm registry (packages) |
| Language version on site | **0.5.2** (spec-stack tip; README's 0.3.0 claim is stale — fix in cleanup) |

## Part 1 — npm distribution

Layout (new, canon-shaped):

```
package.json              # root bun workspace: workspaces: ["packages/*"]
packages/
  cli/                    # npm: lutecli — launcher, bin "lute"
    package.json          # bin {lute: ./bin.js}, optionalDependencies on core-* pkgs
    bin.js                # #!/usr/bin/env node → import ./dist/index.js
    src/index.ts          # platform detection + binary resolution + argv/exit passthrough
    tsconfig.json
  core-darwin-arm64/      # npm: lutecli-core-darwin-arm64, os:[darwin] cpu:[arm64], files:[bin]
  core-linux-x64/         # npm: lutecli-core-linux-x64, os:[linux] cpu:[x64]
```

`tree-sitter-lute/` stays OUT of the workspace (independent npm package with its own lock).

Launcher (`packages/cli/src/index.ts`): adapt canon's line-for-line (which is itself adapted from
tokscale's adversarially-tested logic) — libc-kind probing (gnu/musl), `resolveTargetPackageName`,
search-path order, realpath self-reference guard (fork-bomb prevention). Search order:

1. Workspace dev build: `target/<rust-triple>/release/lute`, then `target/release/lute`
2. Launcher's own bundled `bin/lute`
3. Resolved `lutecli-core-<platform>` optionalDependency across every node_modules topology

Unsupported platform → actionable error naming the supported matrix. Names substituted:
`canon`→`lute`, `canoncli`→`lutecli`.

CI (`.github/workflows/`, adapted from canon): `test.yml` (cargo test + launcher typecheck),
`build-native.yml` (release-matrix cargo builds for darwin-arm64 + linux-x64), `publish.yml`
(stamp binaries into core packages, npm publish all three).

## Part 2 — website

`packages/website/`: Astro + Starlight, `@fontsource` fonts, custom `src/styles/theme.css`,
`vercel.json`. Landing = Starlight splash-template page with custom sections:

1. Hero — lute illustration, tagline ("A total language for visual-novel scenarios" register),
   `bunx lutecli check scene.lute` install snippet
2. `.lute` code showcase (dialogue → match → timeline excerpt, syntax-highlighted)
3. Feature grid: total-not-Turing-complete · typed plugin capabilities · CEL + Datalog facts ·
   **trace: simulate before you ship** · **graph-checked connectivity (reach/envelope)** ·
   **AI-ready authoring surface (`lute context`)** · LSP + editors · compiles to JSON
4. VN dialogue demo section (violet-glow dialogue boxes rendering a sample scene)
5. Footer

### Docs sidebar (full feature surface — nothing dropped)

| Section | Pages |
|---|---|
| Getting Started | installation (`bunx lutecli`) · write your first scene (from `docs/getting-started-first-scene.md`) |
| Language | frontmatter & profiles · dialogue & cast · directives · branch / match / when · choices & hubs · timeline & property tracks · components & extends · params · quests & scenes · imports (`uses:`) |
| State & Logic | 3-tier state model · facts + Datalog · CEL expressions · state schemas |
| Connectivity | scene graph & `after:` · reachability · Guaranteed/Possible envelopes |
| Tooling | CLI reference (check · check-project · compile · trace · scenario · context · tag · fix · catalog) · tracing guide (mock YAML: state/facts/choose/events/accepts) · providers & catalog · editors (LSP) |
| Plugin System | concepts · manifest schemas · bridge directives · profiles |
| Examples | showcase walkthrough (episode01 · hub-demo · when-is-demo) |
| Specification | versioned spec-stack index (0.1.0 → 0.5.2), linking to repo as source of truth |

Content sources: `docs/getting-started-first-scene.md`, `docs/architecture.md`,
`docs/plugin-system.md`, `docs/proposals/**`, `docs/examples/**`, `editors/README.md`,
`crates/lute-cli/src/main.rs` (subcommand contracts incl. exit codes). Site pages are adapted
prose, not verbatim spec dumps; normative source of truth stays in `docs/proposals/`.

ko locale: landing + Getting Started + Concepts overview pages first; remaining pages fall back
to en (Starlight default behavior).

### Image assets

Generated via sonic subagents using `generate_image`: hero lute illustration (dark bg,
amber/gold), OG social card, favicon source. Stored under `packages/website/public/` and
`src/assets/`.

## Implementation slices (parallel)

- **A** — root workspace + `packages/cli` + platform packages + CI workflows
- **B** — website scaffold + theme.css + landing page
- **C** — docs content (en)
- **D** — image assets (sonic + generate_image)
- **E** — ko translations (after C stabilizes page set)

## Verification

- Launcher: `cargo build --release` then `bun packages/cli/src/index.ts check docs/examples/bianca-s01ep02.lute`
  resolves the workspace binary and passes through exit code
- Website: `bun run build` clean; browser smoke of landing + a docs page + locale switch
- CI: workflow files lint (actionlint if available); no publish dry-run against real registry

## Non-goals

- Windows / musl platform packages (add later: new `packages/core-<platform>` + CI matrix row)
- Rendering full normative spec text on the site
- Publishing to npm in this change (workflows land; actual publish is an operator action)
- Dashboard/report site features (canon-specific)
