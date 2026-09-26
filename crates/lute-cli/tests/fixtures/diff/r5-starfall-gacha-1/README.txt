Round-5 LANGUAGE-GAP (0.26.0), closed by dsl 0.27.0 §3: a kind-target beat asks about the member it
was raised for. `holds(owned(occasion.target))` (a ground fact argument) and
`user.bond[occasion.target]` (a `per: hero` family read by the bound member) check, play and test —
0.26.0 refused them with E-CEL-PROFILE / E-UNDECLARED. "Is this a duplicate?" and "what bond rank is
this hero?" are written once for the roster.
Run:
  lute check-project .                              -> ok, 0 warnings
  lute play . --script plays/summon.play.yaml       -> bram new, then bram again (after owned(bram));
                                                       aria's SSR fanfare; bram "almost smiles" at bond
                                                       150, "trusts you now" at 200
  lute test tests --project .                       -> 2 passed (occasion.target mocked)
