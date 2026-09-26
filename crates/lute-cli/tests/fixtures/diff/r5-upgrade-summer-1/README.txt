r5-upgrade-summer-1 — `occasion.target` cannot be a fact-query argument in a kind beat
Verdict: LANGUAGE-GAP (not a regression: kind targets are new in 0.26.0).

Command (lute 0.26.0):
  cd /tmp/r5-upgrade-summer-1 && lute check-project .

Actual:
  ./lore/talks.lute:7:70: error [E-CEL-PROFILE] fact-query patterns take compile-time-ground literals or `_` (dsl 0.3.0 §5/§8)

Expected:
  `target="kind:place"` binds `occasion.target` to one closed-kind member per raise
  (docs: "In the beat's `when`, its guards, and its text, `occasion.target` is the member the
  occasion was raised for ... typed by the kind"), so `holds(at(sol, occasion.target))` is
  ground for every member and could be checked/expanded per member (radio, roof, deck).
  Either accept it (expand per member, like the member ladders `lute beats` already draws),
  or have the message say that `occasion.target` is not usable inside `holds(...)` and show
  the per-member spelling. The website's Kind targets section does not mention the limit.

Workaround (workaround/talks.lute.txt; checks clean, the play passes):
  when="(occasion.target == 'radio' && holds(at(sol, radio))) || (occasion.target == 'roof' && holds(at(sol, roof)))"
  i.e. restating every member by hand — which is the per-member duplication a kind beat is for.

Game impact (Summer Station): Sol's "warm once a day wherever you find him" is two bundle beats
tied by `share="solWarm"` (talks.solWarmRadio / talks.solWarmRoof). One `target="kind:place"`
beat with `when="holds(at(sol, occasion.target))"` and `once="day"` would replace both and the
share key; with the workaround it is longer than what it replaces, so it was not adopted.
