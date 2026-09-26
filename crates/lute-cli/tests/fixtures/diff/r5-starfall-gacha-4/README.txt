TOOL-DEFECT (0.26.0, minor): `lute beats --target hero.cyra` marks member beats cyraA/cyraB
"shadowed" by the kind:limited beat, but prints no verdict for s.goldLight (kind:ssr,
priority 1), which can equally never win for hero.cyra (limitedGlow, priority 2, no when,
once false, is ordered before it). The kind beat is only reachable for other members
(aria), so its per-member row in the member ladder should read shadowed / covered by.
Run: lute check-project . ; lute beats . --target hero.cyra
