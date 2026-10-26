# @lute-lang/lute

The Lute command-line toolchain for `.lute` branching game narratives: the checker, compiler, play and test runners, formatter, migrations, and the `lute-lsp` language server. This package is a small launcher; the native binaries come from the platform package npm installs alongside it (`@lute-lang/lute-core-darwin-arm64`, `@lute-lang/lute-core-linux-x64`, or `@lute-lang/lute-core-win32-x64`).

## Install

```sh
npm install --save-dev @lute-lang/lute@0.51.0
# bun add -d @lute-lang/lute@0.51.0
# or once, globally: npm install -g @lute-lang/lute@0.51.0
```

Then run it through your package manager (`npx lute …`, `bunx lute …`) or, installed globally, as `lute`. The package also installs `lute-lsp` for editors. `lute version` prints the toolchain, language, and IR versions this install implements; `lute doctor` checks the toolchain and the project around you.

## Usage

```sh
lute --help                     # every command, one line each
lute <command> --help           # one command's arguments and exit codes
lute --explain E-BAD-ENUM       # what raises a diagnostic code, and its reference page

lute init my-game               # scaffold a project
lute check --project my-game      # check every document of the project
lute compile --project my-game -o dist/bundle
lute play --project my-game --script my-game/plays/first.play.yaml
lute test --project my-game               # scenario tests and play scripts with `expect:`
lute fix my-game                # mechanical migrations after an upgrade
```

The commands, grouped:

- **Check:** `check`, `check --project`, `lint`, `doctor`, `constraints`, `impact`.
- **Build:** `compile` (one document, or `--project <dir> -o <dir>` for a project bundle with `project.index.json`), `compile --stream`, `expr`.
- **Run and test:** `play`, `test`, `trace`, `run`, `calendar`, `beats`, `scenario`.
- **Author and maintain:** `init`, `new`, `fmt`, `fix`, `tag`, `patch`, `diff`, `context`, `refs`, `lore`, `loc`, `catalog`, `version`.

Most commands exit `0` on success, `1` on a diagnostic error, and `2` on a usage or I/O failure; `trace` and `play` exit `3` for a walk that could not decide something. The [CLI reference](https://lute-lang.vercel.app/tooling/cli/) states each command's codes.

## Documentation

- [CLI reference](https://lute-lang.vercel.app/tooling/cli/) — every command and flag
- [Playing a story (`lute play`)](https://lute-lang.vercel.app/tooling/play/)
- [Diagnostics reference](https://lute-lang.vercel.app/reference/diagnostics/)
- [Installation](https://lute-lang.vercel.app/getting-started/installation/) and [your first scene](https://lute-lang.vercel.app/getting-started/first-scene/)
- [Changelog and migration notes for 0.51.0](https://github.com/journeyWorker/lute/blob/v0.51.0/CHANGELOG.md#0500)
- [Versioning policy](https://github.com/journeyWorker/lute/blob/v0.51.0/docs/versioning.md)

Source, issues, and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
