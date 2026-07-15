---
title: Facts and Datalog
description: The relational fact layer beside Lute's scalar tiers — declared entities and relations, ground facts asserted and retracted as deltas, and a total Datalog derivation layer that stays terminating by construction.
---

Scalar tiers hold magnitudes; they cannot express *relationships between entities* — "Shadowheart is in the party," "the player told Halsin about the grove." Lute adds a **relational fact kernel** beside the scalar tiers: a closed, n-ary fact database over a declared finite vocabulary. A document opts in simply by declaring relations; a document with none behaves exactly as before.

## Entities, relations, ground facts

Entity **kinds** enumerate their members (`members:`) or are engine-open (`open: engine`). A **relation** is a declared predicate with fixed arity and a typed argument signature; each argument ranges over an entity kind, a named enum, or `bool`. A **fact** is a relation applied to matching entities — it is *symbolic* (it holds or it does not; no numeric value slot). Every fact carries a valid-time interval, so retraction is a tombstone, never a deletion.

```yaml
entities:
  character: { members: [shadowheart, halsin, player] }
  location:  { members: [camp, grove, moonrise] }
relations:
  inParty:    { args: [character], tier: run }
  atLocation: { args: [character, location], tier: run, key: [0] }
  canReach:   { args: [character, location], derive: true }
facts:
  - "inParty(shadowheart)"
  - "atLocation(player, camp)"
```

Content writes **deltas** with the leaf directives `::assert` and `::retract`; the engine maintains the cumulative, time-scoped view. A functional `key:` auto-invalidates the superseded fact. Wildcards (`_`) are admitted only in `::retract`.

```lute
::assert{ atLocation(shadowheart, grove) }
::retract{ atLocation(shadowheart, _) }
```

## Datalog derivation

A relation marked `derive: true` is computed by `rules:` — Horn clauses, function-free, with stratified negation. Multiple rules for one head union.

```yaml
rules:
  - "canReach(C, L)  :- atLocation(C, L)"
  - "canReach(C, L2) :- canReach(C, L1), connected(L1, L2)"
```

Because there are no function symbols, the Herbrand base is finite: bottom-up evaluation reaches a least fixpoint in finitely many steps, so **every derivation terminates** — this is Datalog, not Prolog. Safety requires every head/negated/guard variable to appear in a positive body atom; violations are `E-DATALOG-UNSAFE`, a negation cycle is `E-DATALOG-UNSTRATIFIED`, a would-be function term is `E-DATALOG-FUNCTION`. A rule body may carry a scalar CEL guard (`cel("run.act == 1")`) but never a fact query — that firewall (`E-DATALOG-GUARD-FACT`) keeps every dependency visible to the analysis. Derived and `reserved:` relations are read-only to content. The whole layer reduces to data the engine evaluates deterministically; nothing is author-iterated.
