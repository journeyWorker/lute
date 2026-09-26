r5-upgrade-crown-3 — trace/test walk a kind-target entry with no `occasion.target` as "no arm" and pass (Lute 0.26.0)

barks.lute: entry `again` on `bossDefeated`, target="kind:foe", body `<match on="occasion.target">` with
one arm per member (eel, regent) and no `unset` arm — legal, because 0.26.0 says `occasion.target`
is "engine-owned, always assigned" and "`<match on="occasion.target">` needs no `unset` arm".
tests/again.test.yaml presents the entry without `state: { occasion.target: … }`.

Commands:
  lute trace barks.lute --project . --entry again
  lute test . --project .

Actual:
  <match occasion.target>   -> no arm
  trace complete: 1 decision; arms 0/2 (occasion.target @8:3)        (exit 0)
  PASS  ./tests/again.test.yaml                                      (exit 0)

The walk takes a path the checker has ruled impossible (occasion.target unset), prints nothing
from the body, and a `transcriptLacks` test passes vacuously. With a mock it works:
`--state occasion.target=regent` -> `arm 2 (is="regent")`; a non-member mock is
E-TRACE-MOCK-TYPE.

Expected: the walk refuses or halts like any unresolved input it needs — e.g.
  trace incomplete: `occasion.target` of kind-target entry `again` is not mocked — supply
  --state occasion.target=<eel|regent> (exit 3)
and `lute test` fails the test naming the missing `state: { occasion.target: … }` mock
(as it now does for an ineligible beat, T1-7). Walking every member would also be acceptable.

Verdict: TOOL-DEFECT (new feature in 0.26.0; checker and walker disagree on "always assigned").
