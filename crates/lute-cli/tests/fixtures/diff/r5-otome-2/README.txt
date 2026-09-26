Repro (lute 0.26.0): an excludes: pair where one side is derived by a rule whose
cel() guard reads state.
  lute check-project .                 -> ok (no E-FACT-EXCLUSIVE although both hold on the only route)
  lute trace scenes/festival.lute --project .  -> trace complete (no refusal)
  lute test . --project .              -> PASS, and the test asserts BOTH exclusive facts hold
  lute play . --script plays/p.play.yaml -> halts only at the END of the step, after two
                                          more lines played (docs: "right under the write")
Compare: make onRoute a base relation and ::assert{onRoute(ren)} -> play halts right under the assert.
