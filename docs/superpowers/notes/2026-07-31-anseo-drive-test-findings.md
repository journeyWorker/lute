# Anseo drive-test — running findings log

**This file is the primary deliverable.** The Anseo example is the instrument; this
log is the measurement. A task that produces a clean example and an empty section
here has not been executed — it has been evaded.

## What is being measured

Whether Lute 0.9.0 is mature enough to author a real work in. Not whether a
determined agent can eventually make `check-project` exit 0 — that is always true
of a Turing-complete-adjacent toolchain and measures nothing.

## The authoring rule (binding on every task that writes content)

> **Write what the beat needs. Then find out whether Lute can express it.**

Never the reverse. Choosing what to write based on what you already know compiles
produces a green example and a false reading. If a scene wants a character to
interrupt another mid-line, write that first and discover the answer — do not
quietly substitute two sequential lines because you know those work.

## Capture protocol

Append an entry the moment friction occurs, not at the end of the task from
memory. Reconstructed logs lose exactly the near-misses that matter.

Every entry carries:

- **Intent** — what the beat needed, in plain prose, written *before* the language
  enters the picture.
- **Attempt** — the form you reached for first, verbatim.
- **Result** — the exact diagnostic, or the silence.
- **Resolution** — what you ended up writing, or `NONE — intent abandoned`.
- **Verdict** — one of the four below.

### Verdicts

| Verdict | Criterion |
|---|---|
| `LANGUAGE-GAP` | The intent cannot be expressed. You changed the story to fit the tool. |
| `ERGONOMIC` | Expressible, but the working form is materially worse than the natural one — more verbose, more indirect, or split across files for no modelling reason. |
| `DOC-GAP` | Expressible and reasonable, but **you had to read Rust source, a proposal, or a test to find it.** The website docs and `lute context` did not get you there. |
| `AUTHOR-ERROR` | The docs said so plainly and you missed it. Not a finding — record it only if the diagnostic pointed somewhere unhelpful. |

**The `DOC-GAP` bar is deliberately harsh.** A working author cannot read
`lower.rs`. If you needed to, the language failed them even though it compiled.

### Also record, always

- **A diagnostic that misdirected.** It said X, the real problem was Y. This
  outranks almost everything else here: a wrong error message costs an author more
  than a missing feature they can see is missing.
- **Silence.** You wrote something plausible, nothing complained, and it did not do
  what you meant. `exit: true` on a content line was exactly this, found while
  planning. Silence is the most expensive failure mode and the hardest to notice —
  when a beat does not appear in the artifact, log it before fixing it.
- **What worked well.** A maturity assessment that only lists complaints is not an
  assessment. If a construct carried real weight cleanly, say so and say why.

---

## Findings

<!-- Task agents append below. One `### T<N> — <short title>` section per task,
     with entries inside. Never rewrite another task's section. -->
