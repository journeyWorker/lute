Repro (lute 0.26.0): state paths typed { domain: <enum> } are never member-checked.

world.schema.yaml declares three enum paths:
  run.ending: { type: { domain: ending } }   # long-form enum with labels:
  run.flat:   { type: { domain: flat } }     # flat enum list
  run.inline: { type: { enum: [a, b] } }     # inline enum

  lute check scenes/guard.lute
    -> ok: `::set{run.ending = 'tragic'}`, `::set{run.flat = 'zzz'}` and the guards
       `run.ending == 'tragic'`, `run.flat == 'zzz'` all pass.
  Control: cp inline-control.lute.txt scenes/control.lute && lute check scenes/control.lute
    -> E-SET-TYPE and E-WHEN-LITERAL-DOMAIN for the inline-enum path run.inline.

  lute check-project .  -> ok, although scenes/end.lute (via components/ending.component.lute, param typed
                           { entity: outcome }) writing "tragic" into run.ending is `ok`
  lute play . --script plays/p.play.yaml -> `set run.ending = "tragic"` (out-of-domain value stored)

Same holds for into="run.ending" value="tragic" and <when is="tragic"> (probed; not kept).
The cheatsheet recommends { domain: weekday } exactly to get labels:, so a labelled
enum silently loses member checking. In the Lantern Academy game, an ending beat
`when="run.route == 'rne' && …"` (typo) checks clean and `lute beats` gives no verdict.
