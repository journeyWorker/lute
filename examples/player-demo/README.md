# The Signal at Low Tide

A short English visual novel for the Lute 0.41 web player. Mara, Elio, and the
missing Liora follow a lighthouse signal through one foggy day. The story takes
about 10–15 minutes and has dawn and dusk endings. The chapter-one route and the
chapter-two timed crossing both affect trust and the final choice.

## Run

From the repository root:

```sh
cargo build --release -p lute-cli
./target/release/lute check-project examples/player-demo
./target/release/lute play examples/player-demo --script examples/player-demo/tests/dawn.play.yaml
./target/release/lute play examples/player-demo --script examples/player-demo/tests/dusk.play.yaml
./target/release/lute test examples/player-demo/tests/dawn.play.yaml
./target/release/lute test examples/player-demo/tests/dusk.play.yaml
```

The player bindings use a 1280×720 canvas and assets under `assets/`. Regenerate
the deterministic art and audio pack with:

```sh
python3 examples/player-demo/scripts/generate-assets.py
```

The generator uses only the Python standard library (PNG, WAV, gradients,
shapes, and synthesized loops); it does not download anything.
