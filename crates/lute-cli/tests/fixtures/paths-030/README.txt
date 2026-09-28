paths-030 — names as the engine spells them (Lute 0.30).

A quest `zero-coke-001`, places `lab-b2` and `001` with a per-place visit
counter, and a fact `at("lab-b2")`. Conditions reach a name that is not an
identifier by a quoted index — `quest["zero-coke-001"].state`,
`run.visits["lab-b2"]`, `holds(at("lab-b2"))` — and every tool keys on the
canonical dotted path (`run.visits.lab-b2`).
Run: lute check-project . ; lute play . --script plays/p.play.yaml ;
lute test tests/wrap.test.yaml
