---
title: Full-spec showcase
description: A walkthrough of the self-contained showcase project — a full-feature episode plus hub and when-is companions that drive the implemented language surface end-to-end and check clean under 0.8.0.
---

The [`docs/examples/showcase/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase) project is one self-contained scenario that drives the implemented language surface end-to-end — frontmatter and profiles, all four state tiers, schema composition, `<branch>`/`<match>`/`<hub>`, timelines, content components, and a plugin bridge — and checks clean:

```sh
lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase   # exit 0, 0 warnings
```

It ships a complete plugin (`showcase.pack`) covering six of the export kinds — `directives/`, `state/` (shapes + templates), `providers/`, `bridge/`, `assetkinds/`, `defs/` — plus a base/child schema pair, a reusable content component, and a pinned `castId` catalog. Three scene files drive it.

## `episode01.lute` — the full feature map

The episode wires everything together: root `profile` selection with scene-local plugin options, `uses:` schema import with `extends:` composition, all four state tiers, `<branch>`/`<choice>` with `when` guards and the `into=` run-record sugar, `<match>`/`<when>`/`<otherwise>`, a four-track `<timeline>`, and a plugin bridge directive `::serve` whose attrs combine a `providerRef` id with an `assetKind` id.

```lute
::serve{kind="rhythm" performer="bianca_star" poster="PT.bianca_star.0" resultKey="debut" sync="true"}

<match on="scene.serve.debut.rank">
  <when test="$ == 'gold'">
    @bianca{code="0020" emotion="delighted" variant="1"}: A perfect service!
    ::set{scene.affect.bianca += 1}
  </when>
  <when test="$ in ['silver', 'bronze']">
    @bianca{code="0030" emotion="content" variant="0"}: Not bad at all, Mr. Fixer.
    ::set{run.affection += 1}
  </when>
  <otherwise>
    @bianca{code="0040" emotion="shy" variant="0"}: Shall we try once more?
  </otherwise>
</match>
```

## `hub-demo.lute` — revisit hub, `<when is>`, and interpolation

A non-episode companion that both checks clean and *compiles*. It demonstrates a revisit `<hub>` (with `once`, `when`-guarded, and `exit` choices satisfying the no-dead-end obligation), `<when is="…">` literal-pattern arms over the hub's implicit recording enums, and `{{…}}` content interpolation.

```lute
<hub id="chatWithBianca">
  <choice id="askCoffee" label="Ask about the coffee" once>
    @bianca{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" label="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.bianca += 1}
  </choice>
  <choice id="leave" label="Head out" exit>
    @fixer{code="0040"}: I'd better get moving.
  </choice>
</hub>
```

## `when-is-demo.lute` — `<when is>` over a plain enum

The companion to hub-demo: the same `<when is="…">` literal-pattern arms, but over a plain scene-local finite enum, **including an alternation arm**. A default-valued enum is definitely assigned, so full `is` coverage is exhaustive with no `<otherwise>`:

```lute
<match on="scene.mood">
  <when is="calm">
    @fixer{mono}: Steady breathing. Nothing to prove tonight.
  </when>
  <when is="tense">
    @fixer{mono}: Shoulders drawn tight — I should tread carefully.
  </when>
  <when is="joyful|playful">
    @fixer{mono}: Light in the eyes, whichever way it tilts.
  </when>
</match>
```

## What 0.8.0 changed here

All three scenes are restamped `luteVersion: "0.8.0"` — a one-line diff each, and the plugin was not touched — and they still check clean. Beyond the artifact's own `lute` / `irVersion` stamps, two things moved in the compiled output, and neither is anything you author.

**`shots`.** Authored `## ` headings used to be parsed and then discarded — the only authored structure with no IR carrier. They now survive compilation, so `episode01.lute` emits its six section titles beside the command array:

```json
"shots": [
  { "shot": 1, "heading": "Venny's Again" },
  { "shot": 2, "heading": "The Rehearsed Entrance" },
  { "shot": 3, "heading": "The Service Bridge" },
  { "shot": 4, "heading": "The Approach" },
  { "shot": 5, "heading": "What Was Recorded" },
  { "shot": 6, "heading": "Content Gates" }
]
```

**`capabilityVersion`.** The `lute.core` snapshot went from eight directives to nine when `::end` landed, and the hash folds every directive in the snapshot — so every artifact here carries a new stamp. Nothing about `showcase.pack` changed; the number moved because the core vocabulary did.

No `addr` moved. Every shot in the project emits well under 100 addresses, which is precisely the byte-stability the [uniform-width rule](/tooling/runtime-contract/) was designed to preserve.

The showcase does not reach the rest of 0.8.0: there is no `::end`, no `after: active(…)`, no quest document, and no `stampattrs/` export in `showcase.pack`. Those live on [Core directives](/language/directives/), [Quests & scenes](/language/quests-and-scenes/), and [Manifests](/plugins/manifests/).

For the complete project — the plugin manifests, schemas, component, catalog, and the full feature→line map — see the [showcase directory and its README](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase) in the repository.
