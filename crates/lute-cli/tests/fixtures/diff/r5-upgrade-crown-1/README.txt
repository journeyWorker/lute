r5-upgrade-crown-1 — a component param's `default: "@def"` escapes E-MAYBE-UNSET (Lute 0.26.0)

Project: gauge.component.lute declares `fathoms: { type: number, default: "@lastFathoms" }`,
with `lastFathoms: "prev.run.depth * 10"` (prev.run.* is unset before the first newRun).
explicit.lute passes the argument: `::use{component="gauge" fathoms=@lastFathoms}`.
defaulted.lute omits it:          `::use{component="gauge"}`.
Neither use is guarded.

Command:
  lute check-project .
  lute trace defaulted.lute --project .

Actual:
  ./explicit.lute:8:33: error [E-MAYBE-UNSET] state path `prev.run.depth` may be read before it is set, read through `@lastFathoms` ...
  ok: ./defaulted.lute (0 warning(s))
  trace: defaulted.lute ...
      @narrator  The gauge shows {{@lastFathoms}} fathoms.
  trace complete: 0 decisions            (exit 0; the unresolved placeholder is printed raw)

  The same in `lute play` (probe inside The Drowned Crown, a hubVisit scene on the first
  run doing `::use{component="gauge" mood="calm"}`):
      @narrator: The depth gauge on the rail still shows {{@lastFathoms}} fathoms, where the needle stuck.
      ── end: complete (1 step) ──     (exit 0)

Expected (CHANGELOG 0.26.0 Added: "an omitted argument takes it, judged at the `::use` like the
argument it stands for"; components-and-extends.md "the default is judged at the `::use` like the
argument it stands for"):
  defaulted.lute:8:1: error [E-MAYBE-UNSET] state path `prev.run.depth` may be read before it is set,
  read through `@lastFathoms` (the default of gauge's `fathoms`) ...
  and trace refused, as for explicit.lute.

Verdict: TOOL-DEFECT (new in 0.26.0; the feature does not exist in 0.25.1).
