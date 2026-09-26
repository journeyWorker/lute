Repro (lute 0.26.0): `lute scenario knowledge` says a negated rule premise can never
be defeated when the rule binds its variable through an entity-kind atom.

  rule:  routeOpen(S) :- suitor(S), not locked(S)
  scenes/clash.lute asserts locked(ren) (in a choice) and locked(kai).

  lute scenario . knowledge --for festival
    -> "not locked(ren) — always holds (nothing can produce locked(ren): asserted by
        scene `clash` (scenes/clash.lute), but none of it yields these arguments from
        what the project asserts) — cannot be defeated"
    while the direct guard `!holds(locked(ren))` in the same scene is reported correctly:
       "holds unless defeated — defeated when locked(ren) is asserted by scene `clash`".

Controls (probed, reverted): the same rule with a base relation instead of the kind
atom (`routeOpen(S) :- met(S), not locked(S)` with met seeded) or a ground rule
(`routeOpen(ren) :- not locked(ren)`) both report "holds unless defeated" correctly.
Also wrong with an unconditional top-level ::assert{locked(ren)} in clash.lute.
check-project and play are right (locked(ren) is producible; play derives routeOpen accordingly).
