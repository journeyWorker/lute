r5-upgrade-summer-2 — transcriptLacks with a line-attribute needle: the pass flips to a fail (not in
the CHANGELOG Compatibility section), and the miss reports the needle as the "actual" line.
Verdict: TOOL-DEFECT (the miss message) + DOC-GAP (Compatibility omits T3-6).
Not hit by Summer Station itself (its needles carry no attributes); found while checking the
Compatibility section.

Command:
  cd /tmp/r5-upgrade-summer-2 && lute play --script plays/p.play.yaml .
  (0.25.1 comparison: copy the dir, set luteVersion "0.25.1", run /tmp/r5-upgrade-bin/lute-0.25.1 play --script plays/p.play.yaml .)

The scene presents   @sol{emotion="happy"}: Vega, Deneb, Altair.
The play asserts     transcriptLacks: ['@sol{emotion="sad"}: Vega, Deneb, Altair.']

Actual, 0.25.1 (rc 0):
  ── expect: every expectation held ──────────────

Actual, 0.26.0 (rc 1):
  ── expect: 1 missed ──────────────
    ✗ end of play: expect transcriptLacks: expected "@sol{emotion=\"sad\"}: Vega, Deneb, Altair." absent, actual "@sol{emotion=\"sad\"}: Vega, Deneb, Altair." present

Expected:
  1. The miss must show the line that actually matched: `@sol{emotion="happy"}: Vega, Deneb, Altair.`
     (and say the needle's attributes were dropped). As printed, it claims a `sad` line is present
     that the transcript above does not contain.
  2. The attribute-dropping is documented under Changed (T3-6), but a `transcriptLacks` needle with
     attributes can now fail a play/test that passed on 0.25.1. That is a break and belongs in the
     CHANGELOG 0.26.0 Compatibility section (which lists only T1-7, T1-4/T1-5 for tests).
  (Arguably transcriptLacks should keep the attributes: "the sad reading never plays" is a
  statement about the attribute.)
