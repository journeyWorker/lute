Repro (lute 0.26.0): an error in a component body is anchored at an unrelated file.

components/bump.component.lute (imported by every document through defaults: components:)
contains, on line 11:
  ::set{run.aff.@who += 1}

  lute check-project .
    -> ./components/aaa.component.lute:1:1: error [E-CEL-PARSE] component `bump`
       (components/bump.component.lute): `::set` takes no attributes other than one trailing
       `when="…"` … (+2 more callers)
           components/bump.component.lute:0:0: error [E-CEL-PARSE] …
       i.e. anchored at ANOTHER component (the first importer), detail at 0:0 not 11,
       plus E-UNDECLARED "state path `run.aff.`" at the right line.
  lute check components/bump.component.lute -> the same error reported 3 times (1:1, 0:0, 11:21)

The message is also misleading: the problem is `.@who` as a path segment; the supported form
is `::set{run.aff[@who] += 1}` (works, and is checked at each ::use with E-COMPONENT-ARG).
