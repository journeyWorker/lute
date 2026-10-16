# The Signal at Low Tide

A short English visual novel for the Lute web player, with a Korean
translation. Mara, Elio, and the missing Liora follow a lighthouse signal
through one foggy day. The story takes about 10–15 minutes and has dawn and
dusk endings. The chapter-one route and the chapter-two timed crossing both
affect trust and the final choice.

## Run

From the repository root:

```sh
cargo build --release -p lute-cli
./target/release/lute check-project examples/player-demo
./target/release/lute play examples/player-demo --script examples/player-demo/tests/dawn.play.yaml
./target/release/lute play examples/player-demo --script examples/player-demo/tests/dusk.play.yaml
./target/release/lute test examples/player-demo/tests/dawn.play.yaml
./target/release/lute test examples/player-demo/tests/dusk.play.yaml
bun run --cwd packages/player-web demo
```

`check-project` reports one warning, `W-BINDINGS-VERSION-STALE`: the bindings
declare `bindingsVersion: "0.47"` for `ui.title.text` (the title heading),
newer than a 0.47 toolchain's line.

The player bindings use a 1280×720 canvas and assets under `assets/`. Regenerate
the deterministic art and audio pack with:

```sh
python3 examples/player-demo/scripts/generate-assets.py
```

The generator uses only the Python standard library (PNG, WAV, gradients,
shapes, and synthesized loops); it does not download anything.

## Korean translation

`locales/ko.json` is the project's `lute loc export` with every `text`
translated; `locales/bundle.json` is its locale bundle. After changing a line,
choice or prompt, re-export, update the translation and re-import (from this
directory):

```sh
lute loc export . -o /tmp/export.json   # merge new rows into locales/ko.json
lute loc import locales/ko.json -o locales/bundle.json
lute compile --all . --locales locales/bundle.json -o /tmp/bundle
```

The web demo compiles with `--locales locales/bundle.json`; Settings → Locale
switches between `en-US` and `ko`.

## Second play-through

The final chapter counts each voyage that reaches the lantern in `user.voyages`. On the next run,
the first chapter shows the count and, once a run has ended, Mara's
`prev.run`-gated line. The web player keeps the finished run as its lineage,
so both survive a page reload.
