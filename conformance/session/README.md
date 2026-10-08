# Runtime session conformance cases

Each case is a minimal project and `script.play.yaml` used by `lute play`.

- `occasion-first` — first selection and priority tie ordering.
- `occasion-sequence` — sequence selection consumes ordered candidates.
- `occasion-all` — all selection and explicit `pick` answers.
- `quest-settle` — earlier quest document updates are visible to later documents.
- `grant-instances` — grants and run-tier repeatable quest instances.
- `clock-raises` — slot/day clock movement raises occasions.
- `hub-once` — once hub option and nested choice suspension/resumption.
- `snapshot-hub` — hub presentation suitable for snapshot/restore at its choice input point.
- `bridge-call` — bridge await/result and typed effect.
- `new-run` — run-tier reset and queued `nextRun` acceptance.

The `E-RUNTIME-BUSY` rejection cannot be expressed by a play script and is
intentionally omitted. Generated event logs are added separately.
