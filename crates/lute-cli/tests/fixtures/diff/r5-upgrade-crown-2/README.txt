r5-upgrade-crown-2 — `per: <kind>` ignores the members a kind gets from its sub-kinds (Lute 0.26.0)

0.26.0 (CHANGELOG Changed; spec §2.3): "A `subsetOf:` sub-kind's members are members of its parent:
a trainer listed in `trainer` (⊂ `person`) no longer needs a second line in `person`."

world.schema.yaml:
  user.seen: { per: npc }      npc:       { members: [tavi] }
  user.bond: { per: bonded }   bonded:    { subsetOf: npc,    members: [brann] }
                               confidant: { subsetOf: bonded, members: [sefa] }
  relations: close: { args: [npc] }
a.lute asserts close(sefa), close(brann) and sets user.seen.tavi, user.seen.brann, user.bond.brann,
user.bond.sefa.

Command:
  lute check-project .

Actual (0.26.0):
  ./a.lute:11:7: error [E-UNDECLARED] `::set` target `user.seen.brann` is not declared in the `state:` schema (dsl §7.3.4)
  ./a.lute:13:7: error [E-UNDECLARED] `::set` target `user.bond.sefa` is not declared in the `state:` schema (dsl §7.3.4)
  (the relation arguments close(sefa) / close(brann) are accepted: the inherited membership
  works for relation domains — and for occasion target domains, see The Drowned Crown's
  talk@npc.sefa — but not for `per:` families, one level or two)

Expected: clean. brann is a member of npc through bonded, sefa a member of bonded (and npc)
through confidant, so `per: npc` / `per: bonded` declare user.seen.brann / user.bond.sefa.
Or, if `per:` is meant to expand only listed members, the Compatibility note / spec §2.3 should
say so and the error should point at the sub-kind ("`sefa` is a member of `bonded` only through
`confidant`; `per:` expands listed members").

For comparison, 0.25.1 rejects the schema outright (E-ENTITY-KIND-SHAPE "`brann` is not a member
of `npc`"), so this is not a regression — the new inheritance rule is only half applied.

Verdict: TOOL-DEFECT.
