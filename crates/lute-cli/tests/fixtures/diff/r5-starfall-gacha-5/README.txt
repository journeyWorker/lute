TOOL-DEFECT (0.26.0, minor): a step `expect.presented` on an `advance:` step is documented
(clock.md "Moving time", cheatsheet) as judging the `slot` occasion raised where the clock
stops, "exactly as an occasion: step would", and play refuses it outright when the clock has no
slot raise ("the advance presents nothing there"). But with `raise: { slot: dailyReset,
dayEnd: dayClose }`, the actual list it compares also contains the dayEnd beat presented at the
midnight crossed, and the miss is labelled "at dailyReset":
  expected [r.resetLine], actual [r.closeLine, r.resetLine]
Either judge only the slot raise, or (better) judge every presentation of the step and then
also allow it on a dayStart-only clock.
Run: lute play . --script plays/p.play.yaml
