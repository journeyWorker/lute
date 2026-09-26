LANGUAGE-GAP (0.26.0): a user-tier quest cannot run again for an event rerun.
harvestMissions (start="@harvestLive", windows days 3-4 and 8-9) completes in the first window and
stays complete; in the rerun window the play expects it active again and it is complete.
Only a run-tier quest + newRun (or a copy of the quest under a new id) re-arms it.
Run: lute play . --script plays/p.play.yaml
