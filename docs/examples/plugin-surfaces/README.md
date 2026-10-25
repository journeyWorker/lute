# Plugin surfaces (0.50)

A small, self-contained project for the 0.50 authoring-extensibility features. One
plugin, `acme.stage`, declares the columns, tokens and reward kinds of an imagined
spreadsheet port, and a handful of documents use them:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p lute-cli
./target/debug/lute check --project docs/examples/plugin-surfaces   # exit 0
./target/debug/lute test  --project docs/examples/plugin-surfaces   # every test and play passes
./target/debug/lute lint  --project docs/examples/plugin-surfaces   # the node lints, exit 0
./target/debug/lute compile docs/examples/plugin-surfaces/scenes/cafe.lute \
  --project docs/examples/plugin-surfaces                            # stamps in the IR
```

> **This project needs the 0.50 toolchain.** It uses `stampAttrs[].on`, `hostTokens`,
> reward `effects`, `diagnostics:` and `target: node`, none of which an older `lute`
> loads. The guide that explains every recipe here is
> [Porting a content set](https://lute-lang.vercel.app/guides/porting-a-content-set/).

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | `pluginsDir`, the `surfaces` profile, identity, and `diagnostics:` with a `paths:` entry |
| `plugins/acme.stage/plugin.yaml` | the plugin manifest; exports `directives`, `stampAttrs`, `hostTokens`, `rewardKinds` |
| `plugins/acme.stage/stampattrs/columns.yaml` | scoped stamps: `on: [line]`, `on: [bg]` and `on: [showSpeaker]` (one name, two types), `on: [branch, hub]`, and `optionKey` with `required: true` on `choice` |
| `plugins/acme.stage/hosttokens/tokens.yaml` | host tokens `playerName` (`string`) and `rank` (`int`) |
| `plugins/acme.stage/rewardkinds/rewards.yaml` | `BADGE` (`asserts`), `REVOKE` (`retracts`), `OFFER_QUEST` (`accepts`) |
| `plugins/acme.stage/directives/stage.yaml` | `::showSpeaker` (lowered to the core `actor` record) and `::move` |
| `world.schema.yaml` | the `met` relation, the `person` kind, the `anchor` slot and the engine-owned state |
| `scenes/cafe.lute` | scoped stamps on a line, `::bg`, `::showSpeaker`, `<branch>` and `<choice>`; `{{playerName}}` and `{{rank:ordinal}}` |
| `scenes/imported/street.lute` | an imported row whose `W-STAGE-ABSENT` is set `off` for `scenes/imported/**` |
| `quests/visits.lute` | `firstVisit` (grants `BADGE` and `OFFER_QUEST`), `secondVisit` (starts on `met(mira)`, grants `REVOKE`), `giftErrand` (accept-driven, accepted only by a reward) |
| `lute.lint.yaml` | six `node` rules: string members, a dependent-attribute pair on `::move`, and a dependent-key pair on `<branch>`/`<choice>` stamps |
| `tests/*.test.yaml` | the scene's branches, the BADGE and OFFER_QUEST effects, the REVOKE effect |
| `plays/visits.play.yaml` | the reward chain end to end |

## Feature map

| Feature | Where |
|---|---|
| a stamp scoped to content lines | `voiceAssetId` in `columns.yaml`; `scenes/cafe.lute` lines |
| a stamp scoped to a core directive | `assetName` on `::bg` |
| one name, two types on disjoint surfaces | `assetName` on `::bg` (string) and on `::showSpeaker` (int) |
| a stamp scoped to a plugin directive | `assetName` on `::showSpeaker` |
| a stamp on `<branch>` | `menuType` |
| a required stamp on `<choice>` | `optionKey` (`required: true`): every `<choice>` in the project must write it |
| a host token | `{{playerName}}`, `{{rank:ordinal}}`; the runtime keeps the marker verbatim |
| reward effects | `rewardkinds/rewards.yaml`; `quests/visits.lute` |
| a quest gated on a fact a reward asserts | `secondVisit` |
| a quest started by a reward `accepts` | `giftErrand` |
| a project level with `paths:` | `diagnostics:` in `lute.project.yaml` |
| the speaker-staging recipe | `::showSpeaker` before the first line in `cafe.lute` |
| node lints | `lute.lint.yaml` |

`lute check --project` prints the imported scene's `ok:` line with a
`1 suppressed by lute.project.yaml` count: its `W-STAGE-ABSENT` is hidden there only
because the manifest says so. Delete the `diagnostics:` entry and the warning returns.
