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

### T1 — Scaffold and declare

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`.

#### T1.1 — `lute init`'s scaffold checks clean as generated — WORKED WELL

- **Intent** — start a real project from nothing and find out whether the
  scaffolder's output is a working baseline or a thing you must first repair.
- **Attempt** — `lute init docs/examples/anseo`, then immediately, before
  touching a byte: `lute check-project docs/examples/anseo`.
- **Result** — `ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))`,
  exit 0. Six files, and the four that participate in checking are mutually
  consistent: `opening.lute` imports both schemas, uses only members
  `vocabulary.schema.yaml` declares, and sets only a path `world.schema.yaml`
  declares.
- **Resolution** — n/a.
- **Verdict** — worked well. Worth stating plainly because the alternative is
  common and awful: a scaffolder whose first act is to hand you diagnostics.

#### T1.2 — the generated vocabulary is a real starting point, not a stub — WORKED WELL

- **Intent** — judge whether `init`'s `vocabulary.schema.yaml` survives contact
  with a project that has its own content, or gets deleted wholesale.
- **Attempt** — read it, then replace it with Anseo's vocabulary (brief Step 3).
- **Result** — the generated file declares **all seven** slots the compiler
  types (`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`,
  `vfxType`), and its comments teach the two structural rules you would
  otherwise learn from an error: `action` must carry `exits:`, `anchor` must
  carry `default:`, and it says *why* ("the compiler reads those instead of
  guessing from names"). It also states the 0.9.0 ownership model up front —
  "Lute's compiler ships NO members".
- **Resolution** — Anseo's vocabulary is a **member-for-member substitution into
  the generated skeleton**. Seven slots in, seven slots out; both long-form
  slots kept their long form for the same reason the comment gives. I edited
  values and deleted nothing structural.
- **Verdict** — worked well. This is the single most load-bearing thing `init`
  produced. Under 0.9.0 vocabulary ownership, an author who has never heard of
  `exits:` or `default:` is one `E-DOMAIN-UNKNOWN` away from confusion, and the
  scaffold pre-empts it by making the seven slots an edit rather than a
  discovery. Note the phrasing in the file itself — "Declared up front so
  reaching for one is an edit to THIS list rather than an `E-DOMAIN-UNKNOWN`" —
  someone designed this deliberately, and it paid off here.

#### T1.3 — replacing the schemas out from under the placeholder scene: clean, pointed diagnostics — WORKED WELL

- **Intent** — the scaffold's scene is not my story. Replace both schemas and
  find out whether the toolchain tells me the placeholder now references
  vocabulary and state that no longer exist, or whether I get something
  downstream and confusing.
- **Attempt** — replaced `vocabulary.schema.yaml` first, checked; then
  `world.schema.yaml`, checked. Deliberately did *not* delete `opening.lute`
  first, so the dangling references would be live.
- **Result** — exactly the right errors, at the right spans, naming the right
  fix:
  ```
  scenes/opening.lute:15:20: error [E-BAD-ENUM] `delighted` is not a valid value for `emotion` of `::narrator` (expected one of: level, clipped, frayed, hollowed, wry, stricken)
  scenes/opening.lute:16:8: error [E-UNDECLARED] `::set` target `run.greeted` is not declared in the `state:` schema (dsl §7.3.4) (+1 more: 17:17)
  ```
  Both enumerate the legal alternatives or name the schema section to add to.
  The `(+1 more: 17:17)` roll-up is a nice touch — one entry per *problem*, not
  per occurrence.
- **Resolution** — deleted `scenes/opening.lute` per Step 5; both errors cleared.
- **Verdict** — worked well. Schema-edit blast radius is visible and precise.

#### T1.4 — `E-BAD-ENUM` renders a content line's speaker as a `::directive` — DIAGNOSTIC NIT

- **Intent** — n/a; observed while reading T1.3's output.
- **Result** — the offending source is a content line, `@narrator{emotion="delighted"}: …`,
  but the diagnostic calls it ``for `emotion` of `::narrator` ``. There is no
  `::narrator` directive in the language or in the file. An author who trusts the
  message and searches for `::narrator` finds nothing.
- **Resolution** — the span (`15:20`) is correct and lands you on the right
  attribute, so the cost is seconds, not minutes.
- **Verdict** — `AUTHOR-ERROR`-adjacent, recorded only because the protocol asks
  for diagnostics that point somewhere unhelpful. Minor, but it is a
  one-word fix (`@narrator`) and the same code path presumably renders every
  content-line enum error this way.

#### T1.5 — `lute context` cannot answer "what may I write here?" until the file exists — ERGONOMIC

- **Intent** — before authoring `wake.lute`, ask the tool what the authoring
  surface for that scene is. This is the exact moment an author (or an AI) most
  needs the answer: the file is not written yet.
- **Attempt** — `lute context docs/examples/anseo/scenes/wake.lute`
- **Result** — `lute: cannot read …/wake.lute: No such file or directory (os error 2)`, exit 2.
- **Resolution** — ran `lute context` against the *placeholder* scene instead and
  read the surface off that. Works, because the surface is project-resolved, but
  it is an indirection: you must already have a valid document in the project to
  ask what documents in the project may contain. In a project scaffolded by
  `init` there is always one; in a project where you have just deleted the
  placeholder — which Step 5 instructs you to do — there is not.
- **Verdict** — `ERGONOMIC`. The command's own help says it emits the surface
  "an AI needs to WRITE valid Lute against THIS file's project", and it
  "emits regardless of document diagnostics" — it will happily describe a file
  full of errors, but not a file that does not exist yet. A `--project <DIR>`-only
  invocation with no `<FILE>` would close this; the flag already exists.

#### T1.6 — `lute context` gives you the vocabulary but not the grammar — would NOT have been enough to write `wake.lute`

This is the direct measurement of authoring-surface maturity, so it gets the
detail.

- **Intent** — determine, honestly, whether `lute context` alone would have let
  me write `wake.lute` without the brief.
- **Attempt** — `lute context docs/examples/anseo/scenes/opening.lute` (37 lines,
  1363 bytes) and `--json` (14 top-level keys: `assetKinds`, `capabilityVersion`,
  `components`, `deliveryFlags`, `directives`, `entities`, `enums`, `facts`,
  `projectEnums`, `providers`, `relations`, `reservedQuestPaths`, `rules`,
  `stateSchema`).
- **Result — what it gave me, and it is substantial:**
  - all 9 core directives **with their attribute keys** — `auto: character, anchor, action`
    is precisely what I needed for line 10 of `wake.lute`;
  - all 7 project enums with every member, live against the schema I had just
    written (`anchor: port, center, starboard`) — so it is genuinely
    project-resolved, not a static core dump;
  - the state schema, entities, relations, and the derived rule, rendered in a
    compact readable form (`can_halt/1(crew) [derive]`);
  - the three delivery flags **with prose glosses** (`{mono}: interior monologue / thought (not spoken aloud in-scene)`)
    — the one place in the whole output that explains a *form* rather than
    listing a *value*;
  - `capabilityVersion`, which is the right thing to pin a harness on.
- **Result — what it left out, all of which `wake.lute` needs:**
  1. **The content-line form itself.** Nothing in the output says a spoken line
     is `@speaker{attrs}: text`. `emotion` appears under `projectEnums` with its
     six members, but nothing connects it to any construct — the `directives`
     block lists attribute keys per directive, and `emotion` is not among them,
     because it is a *line* attribute. So the output tells you `clipped` is a
     legal `emotion` while never telling you where an `emotion` may be written.
  2. **`code`.** Absent entirely — not in the human outline, not in the JSON
     (`grep '"code"'` over the JSON surface: no match). Yet `code` is the
     author-supplied half of every `lineId` and `voiceKey`, it is the one
     attribute in `wake.lute` with no vocabulary backing it, and the
     zero-padded-by-tens convention (`0010`, `0020`) is nowhere either. An
     author working from `context` alone writes lines with no `code` — which
     checks clean, and silently yields unstable identity.
  3. **Frontmatter.** No `kind:`, `character:`, `season:`, `episode:`, and —
     most damaging — no `uses:`. `uses:` is the mechanism that puts the enums
     and state the output is *describing* into scope. The surface describes the
     contents of a room without mentioning the door.
  4. **Section/shot headings.** `## Cold Wake` has no representation.
  5. **`enums (0):` next to `projectEnums (7):`** — two enum sections, the first
     empty and unexplained. Reading top-to-bottom, "enums (0)" is the first
     thing that looks like an answer to "what emotions may I use?" and it is the
     wrong one.
- **Resolution** — wrote `wake.lute` from the brief. From `context` alone I
  could have produced the `::auto` line correctly and every *value* correctly,
  and would have had to guess the frontmatter, the `@speaker{…}:` form, the
  heading, and `code` — i.e. the whole grammar.
- **Verdict** — `DOC-GAP`. The command is well-built and its project resolution
  is exactly right; the gap is one of category. It is a complete **vocabulary**
  surface and not an **authoring** surface, and its help text promises the
  latter. The cheapest honest fix is not to add grammar to `context` but to stop
  claiming it: what an AI needs to write valid Lute is this output *plus* a
  form reference, and nothing in the output says so.

#### T1.7 — the `identity:` block is documented only as an error-code entry — DOC-GAP

- **Intent** — write `identity:` templates that fix `lineId`/`voiceKey` for the
  whole eleven-scene work, and find out how discoverable the closed token set is.
- **Attempt** — the brief supplied the block, so I probed discoverability
  independently, both reactively and proactively.
- **Result — reactive discovery is excellent.** A scratch project with a bogus
  token:
  ```
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  ```
  Exit 1. The message names the offending token, the offending template key, and
  **enumerates the entire closed set**. This is a model diagnostic.
- **Result — proactive discovery fails.** Searching the shipped website
  (`packages/website/src/content/docs`) for the token syntax returns exactly one
  hit, `tooling/ai-harness.md:108`, and it is the `E-IDENTITY-TEMPLATE` **error-code
  reference** — i.e. the same information as the diagnostic, filed where you look
  after you have already failed. Specifically:
  - **no worked example of the `identity:` block exists on any authoring page**
    — searching for `voiceKey` across the docs finds it only as a *field in
    compiled JSON output* (`getting-started/first-scene.md`, `plugins/manifests.md`,
    `tooling/runtime-contract.md`), never as a manifest key you may set;
  - **`{prefix}`'s derivation is documented nowhere.** That it expands to
    `{character}.s{season}ep{episode}`, drawn from *scene frontmatter* and
    zero-padded to two digits, appears in no prose. It is inferable only by
    pattern-matching the string `"mira.s01ep01.mira_0010"` in a sample artifact
    against that page's frontmatter. I confirmed the rule empirically instead —
    `character: narrator, season: 1, episode: 1` → `narrator.s01ep01`;
    `character: anseo` → `anseo.s01ep01` — which is the right way to confirm it
    and the wrong way to learn it;
  - `lute init` does **not** scaffold an `identity:` block, and `lute context`
    does not surface identity at all, so neither entry point mentions the
    feature's existence.
- **Resolution** — used the brief's block verbatim; it compiled first try (T1.8).
- **Verdict** — `DOC-GAP`, and a clean instance of the harsh bar. Everything an
  author needs is *technically* present, but reaching it requires knowing the
  feature exists, guessing the YAML shape, and reading an error-code appendix or
  a 0.8.0 proposal. `docs/proposals/scenario-dsl/0.8.0.md` is the only file in
  the repo outside the website that documents it — a proposal, explicitly named
  in the verdict table.
- **Mitigating and worth saying:** the **defaults already are** `{prefix}.{speaker}_{code}`
  and `{speaker}-{code}`. I verified this by compiling the untouched scaffold
  before adding any block: `"lineId": "narrator.s01ep01.narrator_0010"`. So an
  author who never discovers the feature still gets sane, stable identity. The
  gap costs you *control*, not correctness.

#### T1.8 — identity verification: exact, first try — WORKED WELL

- **Attempt** — brief Step 6, `compile` + grep.
- **Result** —
  ```
  "lineId": "anseo.s01ep01.vesna_0010"
  "lineId": "anseo.s01ep01.vesna_0020"
  "voiceKey": "vesna-0010"
  "voiceKey": "vesna-0020"
  ```
  Both `lineId`s match the expected values exactly; `voiceKey` (not asked for,
  checked anyway) matches its template too.
- **Verdict** — worked well. No adjustment to the template was needed or made.

#### T1.9 — a mock left pointing at deleted state rots silently under `check-project` — SILENCE

- **Intent** — none authorial; this is the failure mode the protocol says is
  most expensive, and it fell out of Step 3–5. Task 1 replaces `world.schema.yaml`
  and deletes `opening.lute` while leaving `mocks/playthrough.yaml` as the
  scaffolder wrote it — by instruction.
- **Attempt** — after the schema swap and the deletion, `lute check-project docs/examples/anseo`.
- **Result** — `ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))`,
  exit 0. The mock still reads:
  ```yaml
  # Trace mock (dsl 0.4.0 §4.3) for scenes/opening.lute. Preview with:
  #   lute trace scenes/opening.lute --mock mocks/playthrough.yaml
  state:
    run.greeted: false
  ```
  Both a state path that no longer exists **and** two references to a scene file
  that no longer exists. `check-project` says nothing — mocks are not in its
  walk, so the project is green with a broken mock in it.
- **The error does exist, one command over.** `lute trace scenes/wake.lute --mock mocks/playthrough.yaml`:
  ```
  docs/examples/anseo/scenes/wake.lute:0:0: error [E-TRACE-MOCK-UNDECLARED] `--state run.greeted=…` names a state path not declared in the resolved schema (state-by-typo MUST fail in mocks exactly as in documents, dsl 0.4.0 §4.3, 0.1 §11.1.1)
  ```
  So the rule is implemented and stated forcefully — "MUST fail in mocks exactly
  as in documents" — but it only fires when a human happens to run `trace` with
  that pairing. Nothing pairs them automatically, and nothing in the green
  `check-project` hints that an unpaired mock is sitting there.
- **Resolution** — none. Left as instructed; recorded rather than fixed.
- **Verdict** — `ERGONOMIC`, shading toward the protocol's *silence* category.
  The scaffolder emits a mock; the checker cannot see it; the mock's only
  validator is a command you must remember to run with the right two arguments.
  In an eleven-scene project this is how a `mocks/` directory quietly becomes
  fiction. A project-wide `check-project` pass over `mocks/*.yaml` (or a
  `W-MOCK-ORPHANED`) would close it.
- **Secondary, and a genuine misdirect:** the diagnostic's position is
  `scenes/wake.lute:0:0`. The defect is in `mocks/playthrough.yaml` at line 4.
  It is rendered "exactly like check diagnostics" — which here means it is
  rendered as a *source* diagnostic against a file that is not at fault, at the
  impossible position `0:0`. The message body names `run.greeted`, so you
  recover, but the filename and span both point away from the problem. Per the
  protocol this outranks most of what is above it.

#### T1.10 — nested projects resolve to the nearest manifest — WORKED WELL, and load-bearing here

- **Intent** — `docs/examples/anseo` is a project *inside* the `docs/examples`
  project, and acceptance requires both `check-project docs/examples/anseo` and
  `check-project docs/examples` to pass. I need to know which manifest governs
  Anseo's scenes when the outer root is the one being walked — otherwise the
  `identity:` block I just wrote is decorative for ten of the eleven scenes.
- **Attempt** — three probes. (a) `check-project docs/examples` and confirm the
  nested scene is actually walked. (b) A scratch nested pair in `/tmp` with a
  deliberately invalid `identity:` token in the *inner* manifest, checked from
  the *outer* root. (c) The same pair with a valid but distinctive
  `lineId: "OUTER-{prefix}…"` in the outer manifest only, compiling an inner
  scene.
- **Result** —
  (a) `ok: docs/examples/anseo/scenes/wake.lute` appears in the outer run; 31
  `ok:` lines, `30 file(s)`. The nested scene is walked, not skipped.
  (b) The outer walk surfaces the inner manifest's error verbatim and exits 1 —
  so nested manifests are *discovered*, not ignored.
  (c) The inner scene compiles to `"lineId": "narrator.s01ep01.narrator_0010"`,
  with no `OUTER-` prefix. **Nearest manifest wins.**
- **Verdict** — worked well, and quietly important. Anseo can be a
  self-contained project with its own vocabulary, world, and identity templates
  while living inside the shared examples tree, and neither root contaminates
  the other. Every later task can rely on this.

#### T1.11 — environment note, not a language finding

The editor LSP in this workstation reported five errors on a `wake.lute` that
the CLI checks clean — including `E-SHOT-HEADING` on `## Cold Wake`,
`E-UNCLASSIFIED` on both valid `@vesna{…}:` lines, and an `anchor` domain of
`left, center, right` (the *scaffold's* members, which I had already replaced).
Cause: `/usr/local/bin/lute-lsp` is an **unrelated product** — `lute --version`
there prints `[deprecated] 'lute' is now 'bard lute'` and `0.1.0`. A name
collision, not a Lute defect. Recorded only because an author who installs
"lute" tooling by name can end up with a language server that confidently
contradicts the compiler, and nothing in either tool says why.

#### T1 summary

Eleven entries: five *worked well*, two `DOC-GAP`, two `ERGONOMIC` (one of them
the silent-mock case), one diagnostic nit, one environment note. Nothing in
Task 1 was inexpressible — every construct the brief asked for compiled, and the
identity chain landed exactly on `anseo.s01ep01.vesna_0010` first try. The
friction is uniformly *informational*: the tool knows things (the closed token
set, the seven slots, the derivation of `{prefix}`) that it will tell you only
after you have guessed wrong, and `lute context` — the one command explicitly
built to tell you in advance — describes values without describing forms.
