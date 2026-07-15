---
title: Full-spec showcase
description: A walkthrough of the self-contained showcase project — a full-feature episode plus hub and when-is companions that exercise every implemented Lute feature and check clean.
---

The [`docs/examples/showcase/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase) project is one self-contained scenario that exercises **every implemented Lute feature** end-to-end and checks clean:

```sh
lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase   # exit 0, 0 warnings
```

It ships a complete plugin (`showcase.pack`) exporting all six kinds — `directives/`, `state/` (shapes + templates), `providers/`, `bridge/`, `assetkinds/`, `defs/` — plus a base/child schema pair, a reusable content component, and a pinned `castId` catalog. Three scene files drive it.

## `episode01.lute` — the full feature map

The episode wires everything together: root `profile` selection with scene-local plugin options, `uses:` schema import with `extends:` composition, all four state tiers, `<branch>`/`<choice>` with `when` guards and `persist="run"` sugar, `<match>`/`<when>`/`<otherwise>`, a four-track `<timeline>`, and a plugin bridge directive `::serve` whose attrs combine a `providerRef` id with an `assetKind` id.

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
  <when is="calm">          @fixer{mono}: Steady breathing. Nothing to prove tonight. </when>
  <when is="tense">         @fixer{mono}: Shoulders drawn tight — I should tread carefully. </when>
  <when is="joyful|playful"> @fixer{mono}: Light in the eyes, whichever way it tilts. </when>
</match>
```

For the complete project — the plugin manifests, schemas, component, catalog, and the full feature→line map — see the [showcase directory and its README](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase) in the repository.
