# Runtime session conformance cases

Each case contains:

- `project/`: the complete project and `script.play.yaml` input;
- `expected.jsonl`: generated compact `lute play --events` output;
- `inputs.jsonl`: generated seed/input envelopes extracted from that stream.

Use `conformance/regenerate.sh` to regenerate both JSONL files; never edit
them by hand. The conformance test validates the CLI stream and replays the
inputs through the public runtime API.

- `occasion-first` — first selection and priority tie ordering.
- `occasion-sequence` — sequence selection consumes ordered candidates.
- `occasion-all` — all selection and explicit `pick` answers.
- `quest-settle` — earlier quest document updates are visible to later documents.
- `grant-instances` — grants and run-tier repeatable quest instances.
- `clock-raises` — slot/day clock movement raises occasions.
- `hub-once` — once hub option and nested choice suspension/resumption.
- `snapshot-hub` — hub presentation suitable for snapshot/restore at its choice input point.
- `bridge-call` — bridge await (the call's `attrs`, typed `results`), result, plugin record `attrs`, and typed effect.
- `new-run` — run-tier reset and queued `nextRun` acceptance.
- `timed-choice` — timed branch answered by timeout takes its default.
- `stage-snapshot` — runtime stage across bg/music stop/actor emotion/CG and snapshot/restore.
- `clock-writes` — an `advance:` step's `engine:` writes land before the clock moves: the
  `dayStart` beat reads them, its presentation precedes the closing `clock` event.

The `E-RUNTIME-BUSY` rejection cannot be expressed by a play script and is
intentionally omitted. A gate refusal (`E-OCCASION-GATE`) is not a session case
either — a case runs to exit 0 with one output per input; the rejection line is
in `contract-fixtures.jsonl` and the runtime/`--events` tests
(`crates/lute-cli/tests/runtime_gate_041.rs`).
