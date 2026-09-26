TOOL-DEFECT (0.26.0): `per: <kind>` ignores members a sub-kind contributes.
dsl 0.26.0 §2.3: "A subsetOf: child's members are members of the parent".
`hero: { members: [] }` + `ssr: { subsetOf: hero, members: [aria] }`:
  - user.birthday map default naming aria -> E-STATE-DECL "aria is not a member of entity kind hero []"
  - (variant /tmp/r5g-t3b: no map default) reading user.bond.aria -> E-UNDECLARED
Restating aria in hero's members makes both clean.
Run: lute check-project .

variant-silent-rule/: ivo listed only in `sr` (subsetOf hero). check-project is CLEAN, yet
`hero(ivo)` holds (kind atom, targets hero.ivo accepted) while `user.bond.ivo` does not exist:
the rule `bonded(H) :- hero(H), cel("user.bond[H] >= 100")` instantiates for ivo over an
undeclared path and play --explain shows `? cel("user.bond.ivo >= 100") (undecided)`; an
engine: step writing user.bond.ivo is refused as "not a declared state path". Membership is
inconsistent across per:, kind atoms and targets, and nothing reports it.
Run (in variant-silent-rule): lute check-project . ; lute play . --script plays/p.play.yaml --explain "bonded(ivo)"
