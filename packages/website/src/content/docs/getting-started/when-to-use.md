---
title: Is Lute right for you?
description: Honest selection guidance — the projects and teams Lute fits well today, and the ones it does not fit yet, so you can decide before you invest.
---

Lute is a focused tool, not a general-purpose engine. It is the **author-time
half** of a narrative game: a grammar, a checker, and a compiler that emit a
versioned JSON IR plus CEL for your engine to play. That focus makes it excellent
for some projects and a poor fit for others. This page is the honest version of
that trade-off — read it before you invest, not after.

## Lute fits well when…

- **Your game is branchy and state-heavy.** If choices fork, converge, and gate
  on world state — and you worry about softlocks and unreachable content — Lute's
  whole reason for existing is to prove [reachability](/connectivity/reachability/)
  and [state-availability envelopes](/connectivity/envelopes/) before you ship.

- **You are building a quest-and-dialogue RPG.** Quests with derived completion,
  fact-guarded interrogations, and `after:`-sequenced scenes are first-class. The
  [investigation tutorial](/getting-started/build-an-investigation/) is exactly
  this shape.

- **A team collaborates through Git.** `.lute` files and their schemas are
  plain, diffable text with no binary project file. Branches, reviews, and merges
  work the way your engineers already expect.

- **You want AI to help author, with a checker keeping it honest.** `lute context`
  emits the authoring surface a model needs, and `lute check --json` plus `--deny`
  give a machine-readable pass/fail loop. See the
  [AI harness guide](/tooling/ai-harness/). The checker is the ground truth the
  generator cannot talk its way past.

- **You need one narrative source across multiple engines.** Lute never runs your
  game; it compiles to a documented [runtime contract](/tooling/runtime-contract/).
  One `.lute` project can feed any engine that implements that contract, so the
  story is not welded to a single runtime.

## Lute does not fit yet when…

- **Your team authors only in a node editor.** Lute is a text language. If your
  writers will not work in files and a terminal — and want a purely visual
  drag-and-connect surface — Lute is not that today.

- **You want an install-and-play Unity or Godot product.** There is no drop-in
  runtime plugin. Playing Lute means implementing the runtime contract for your
  engine. That is a bounded, documented job, but it is a job — budget for it.

- **You need general-purpose scripting.** Lute is deliberately **total, not
  Turing-complete**: every scenario provably terminates, which is what lets the
  checker analyze paths at all. Arbitrary loops, open computation, and
  general logic belong in your engine, not in `.lute`.

- **Your project is tiny and strictly linear.** A short, single-path visual novel
  with no branching gains little from reachability and envelope analysis. The
  ceremony of schemas and projects will outweigh the safety it buys.

- **You have no capacity to build a runtime adapter.** If nobody on the team can
  implement (or maintain) the engine-side contract, the compiled IR has nowhere
  to run. Confirm that capacity exists before adopting.

## How to decide

Frame it as selection, not compromise: Lute trades general-purpose flexibility
for **provable guarantees about branching narrative**. If those guarantees are
what keep you up at night — and you can pay the text-authoring and
runtime-adapter costs — Lute is built for you. If your project is linear, visual-
first, or needs open-ended computation, reach for a tool aimed at that instead.

Still unsure? Walk the [first scene](/getting-started/first-scene/) and the
[investigation tutorial](/getting-started/build-an-investigation/) — a couple of
hours there tells you more than any checklist. Then pick a
[learning path](/getting-started/learning-paths/) for your role.
