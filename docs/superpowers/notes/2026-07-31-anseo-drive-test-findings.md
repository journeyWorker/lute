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
- **Verdict** — exactly one of the five below. Never invent a verdict or hyphenate a
  hybrid (`AUTHOR-ERROR-adjacent` is not a verdict); if none fits, say so in the entry
  and raise it with the controller, who owns this table.

### Verdicts

| Verdict | Criterion |
|---|---|
| `LANGUAGE-GAP` | The intent cannot be expressed. You changed the story to fit the tool. |
| `ERGONOMIC` | Expressible, but the working form is materially worse than the natural one — more verbose, more indirect, or split across files for no modelling reason. |
| `DOC-GAP` | Expressible and reasonable, but **you had to read Rust source, a proposal, or a test to find it.** The website docs and `lute context` did not get you there. |
| `AUTHOR-ERROR` | The docs said so plainly and you missed it. Not a finding — record it only if the diagnostic pointed somewhere unhelpful. |
| `TOOL-DEFECT` | The language and its docs are fine; a *tool* is wrong, incomplete, or lying about its own contract. A misdirecting diagnostic, a false green, a capability surface that omits something it advertises. Distinct from `DOC-GAP`: the information exists, but the tool that promised to hand it to you did not. |

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

#### T1.4 — `E-BAD-ENUM` renders a content line's speaker as a `::directive` that does not exist — TOOL-DEFECT

- **Intent** — n/a authorially; first seen in T1.3's output, then probed
  deliberately, because a diagnostic naming a construct the language does not
  have is the protocol's highest-priority category.
- **Attempt** — reproduced from scratch, outside Anseo, so this entry reruns
  without the example tree:
  ```console
  $ ./target/debug/lute init /tmp/t14/proj
  $ cp docs/examples/anseo/vocabulary.schema.yaml /tmp/t14/proj/vocabulary.schema.yaml
  $ ./target/debug/lute check-project /tmp/t14/proj
  ```
  The offending source is the scaffolder's own line 15, untouched:
  `@narrator{emotion="delighted"}: Welcome to your new Lute project.`
- **Result** — exit 1:
  ```
  /tmp/t14/proj/scenes/opening.lute:15:20: error [E-BAD-ENUM] `delighted` is not a valid value for `emotion` of `::narrator` (expected one of: level, clipped, frayed, hollowed, wry, stricken)
  failed: /tmp/t14/proj/scenes/opening.lute (1 error(s), 0 warning(s))
  failed: /tmp/t14/proj (1 file(s), 0 project-wide error(s), 0 project-wide warning(s))
  ```
  There is no `::narrator` directive in the language, in the file, or in the
  nine-directive list `lute context` prints (T1.6). An author who trusts the
  message and searches for `::narrator` finds nothing.
- **Second probe — it is the renderer, not this line.** One scratch file carrying
  a real directive and a content line, each with one bad enum value. Same
  frontmatter as the scaffold's scene, `episode: 2`, saved as
  `/tmp/t14/proj/scenes/probe.lute`:
  ```lute
  ## Probe

  ::auto{character="narrator" anchor="nowhere" action="brace"}
  @narrator{action="jitter"}: A line with a bad action.
  ```
  `./target/debug/lute check /tmp/t14/proj/scenes/probe.lute --project /tmp/t14/proj`:
  ```
  /tmp/t14/proj/scenes/probe.lute:15:37: error [E-BAD-ENUM] `nowhere` is not a valid value for `anchor` of `::auto` (expected one of: port, center, starboard)
  /tmp/t14/proj/scenes/probe.lute:16:19: error [E-BAD-ENUM] `jitter` is not a valid value for `action` of `::narrator` (expected one of: brace, drift, turn-away, seal, unseal, step-out, go-under)
  ```
  `::auto` is right; `::narrator` is fabricated. The `::` is prefixed to the
  owning node's name unconditionally, so this is how *every* content-line enum
  error renders — `action` as much as `emotion` — not a one-off in T1.3's output.
- **Resolution** — none needed. The spans (`15:20`, `16:19`) are correct and land
  on the offending attribute, so the cost is seconds, not minutes.
- **Verdict** — `TOOL-DEFECT`. The language is fine and so are its docs:
  `language/dialogue-and-cast.md` opens by stating the content-line form
  `@speaker{attributes}: the text they say`. A *tool* is describing the author's
  source as a construct that does not exist. Small in cost, but it is a one-word
  fix (`@narrator`) on a shared code path, and it is the cheap kind of wrong — a
  message that invents vocabulary the author will then go looking for.

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

#### T1.6 — `lute context` gives you the vocabulary but not the grammar it advertises — TOOL-DEFECT

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
     checks clean, and silently yields *positional* identity. Verified on a
     scratch project: two bare `@narrator:` lines compile to `…narrator_0010`
     and `…narrator_0020`; insert one line above them, recompile, and those two
     unchanged lines become `_0020` and `_0030`.
     `language/dialogue-and-cast.md` is accurate and careful here — a missing
     `code` "is back-filled deterministically at compile time and can be
     persisted with `lute tag`", i.e. deterministic per compile, not stable
     across edits, which is why `lute tag` exists at all. `context` mentions
     neither `code` nor `lute tag`.
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
- **Verdict** — `TOOL-DEFECT`, not `DOC-GAP`. Every form listed above is
  documented on the shipped website, and I checked each before assigning this:
  - the **content-line form** — `language/dialogue-and-cast.md`, which opens
    "Every content line has the same shape: `@speaker{attributes}: the text they
    say`", then names the line attributes with `code` first;
  - the **frontmatter block** — `language/frontmatter-and-profiles.md`, "Every
    `.lute` document opens with a **YAML frontmatter block** delimited by two
    `---` lines", with `kind`/`character`/`season`/`episode` in its worked example;
  - **`uses:`** — `language/imports.md`, whose title is literally "Imports
    (uses:)" and whose first paragraph names it as the import mechanism;
  - the **`## ` heading** — `getting-started/first-scene.md`, which teaches it
    through `E-CONTENT-OUTSIDE-SHOT` and states the rule as "all content lives
    under a heading".

  So the `DOC-GAP` bar is not met, and claiming it inflated the reading: I did
  not have to open Rust, a proposal, or a test, and a working author would not
  have to either. What failed is the tool's own contract. `lute context --help`
  reads: *"Emit the project-resolved AUTHORING SURFACE for a `.lute` file — the
  directives/attrs/enums/asset-kinds/providers/state-schema/components +
  capabilityVersion an AI needs to WRITE valid Lute against THIS file's
  project."* Read closely, the noun list is honest — every item in it is
  vocabulary, and every item in it is delivered, well. The overclaim is the
  purpose clause. What the output contains is not what an AI needs to write valid
  Lute; it is precisely the half the docs deliberately *delegate* to it.
  `dialogue-and-cast.md` makes that division explicit from the other side —
  "Their *domains* are project vocabulary, not grammar — run `lute context
  <file>` to list the legal `emotion`/`variant` values for your project." The
  docs own the forms and hand off the values; `context` owns the values and
  claims the whole surface.

  That is the `TOOL-DEFECT` criterion exactly: the information exists, and the
  tool that promised to hand it to you did not. It is also worse in practice than
  a documentation hole would be, because the output gives no signal that anything
  is missing — no cross-reference, no form section, not even an explanation of
  the empty `enums (0)`. It reads complete, and a harness pointed at it (which
  the help text invites) has no way to discover otherwise. The cheapest honest
  fix is still not to add grammar to `context`, but to stop claiming it in
  `--help` and to have the output name the pages that carry the forms.

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

#### T1.10 — which manifest governs a nested project is decided by the root you invoke, not by proximity — TOOL-DEFECT

Re-run from scratch during the fix pass, because the original entry described its
probes only in prose. **The re-run overturns its conclusion.** The original read
"nearest manifest wins" and was filed *worked well*; nearest manifest does not
win. What follows is what the commands actually print.

- **Intent** — `docs/examples/anseo` is a project *inside* the `docs/examples`
  project, and acceptance requires both `check-project docs/examples/anseo` and
  `check-project docs/examples` to pass. I need to know which manifest governs
  Anseo's scenes when the outer root is the one being walked — otherwise the
  `identity:` block I just wrote is decorative for ten of the eleven scenes.

- **Attempt (a) — is the nested scene walked at all?**
  ```console
  $ ./target/debug/lute check-project docs/examples
  ```
- **Result (a)** — exit 0, closing with
  `ok: docs/examples (30 file(s), 5 project-wide warning(s))`, 31 `ok:` lines, and
  among them:
  ```
  ok: docs/examples/anseo/scenes/wake.lute (0 warning(s))
  ```
  The nested scene is walked, not skipped. This part of the original entry holds.

- **Attempt (b) — is a nested manifest discovered?** A two-manifest scratch tree,
  built entirely by `lute init` so it pastes and runs:
  ```console
  $ lute init /tmp/nest && lute init /tmp/nest/inner
  $ printf '\nidentity:\n  lineId: "{scene}.{speaker}_{code}"\n' >> /tmp/nest/inner/lute.project.yaml
  $ lute check-project /tmp/nest
  ```
  `{scene}` is not a legal token (T1.7), and it is in the **inner** manifest only.
  Baseline before the edit: `lute check-project /tmp/nest` is exit 0 with both
  scenes `ok:`.
- **Result (b)** — exit 1, and this is the entire output:
  ```
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  ```
  So nested manifests *are* read and validated by `check-project`: a broken one
  two directories down fails the outer run. Note what the outer run does **not**
  say — which manifest. The line is byte-identical to what
  `lute check-project /tmp/nest/inner` prints, carries no path, and the walk emits
  no `ok:` lines before it. In a tree with two manifests you are told a manifest
  is broken and left to find out which. (Recorded here rather than as its own
  entry: same defect class as T1.9's `0:0` span — a project-level diagnostic with
  no usable location.)

- **Attempt (c) — whose templates land in the artifact?** The original entry's
  probe, done properly: give the two manifests *mutually distinguishable*
  templates instead of testing one against a default.
  ```console
  $ lute init /tmp/nest3 && lute init /tmp/nest3/inner
  # append to /tmp/nest3/lute.project.yaml:
  #   identity:
  #     lineId: "OUTER-{prefix}.{speaker}_{code}"
  # append to /tmp/nest3/inner/lute.project.yaml:
  #   identity:
  #     lineId: "INNER-{prefix}.{speaker}_{code}"
  # and in /tmp/nest3/inner/scenes/opening.lute, so the two scaffolds do not
  # collide on episode key: character: narrator  ->  character: inner
  $ lute compile --all --project /tmp/nest3 -o /tmp/nest3out
  ```
- **Result (c)** — exit 0, `lute compile --all: 2 document(s) -> /tmp/nest3out`,
  and every `lineId` in both artifacts — including the **nested** project's —
  carries the outer template:
  ```
  "lineId": "OUTER-inner.s01ep01.narrator_0010"
  "lineId": "OUTER-inner.s01ep01.narrator_0020"
  "lineId": "OUTER-narrator.s01ep01.narrator_0010"
  "lineId": "OUTER-narrator.s01ep01.narrator_0020"
  ```
  `INNER-` appears nowhere. Compiling at the inner root gives it back, and
  single-file compiles follow `--project`, never proximity:
  ```console
  $ lute compile --all --project /tmp/nest3/inner -o /tmp/nest3in
  "lineId": "INNER-inner.s01ep01.narrator_0010"

  $ lute compile /tmp/nest3/inner/scenes/opening.lute --project /tmp/nest3        -> "OUTER-inner.s01ep01.narrator_0010"
  $ lute compile /tmp/nest3/inner/scenes/opening.lute --project /tmp/nest3/inner  -> "INNER-inner.s01ep01.narrator_0010"
  $ lute compile /tmp/nest3/inner/scenes/opening.lute                             -> "inner.s01ep01.narrator_0010"
  ```
  The last is the default template: with no `--project`, **no** manifest is
  consulted — not even the one sitting in the file's own project directory.

- **Attempt (d) — do the checker and the compiler agree?** The (c) tree with the
  inner manifest's template set to the illegal `"{scene}.{speaker}_{code}"` from
  (b), everything else unchanged.
- **Result (d)** — they do not:
  ```console
  $ lute check-project /tmp/nest4
  lute: E-IDENTITY-TEMPLATE: unknown token `{scene}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
  # exit 1

  $ lute compile --all --project /tmp/nest4 -o /tmp/nest4out
  lute compile --all: 2 document(s) -> /tmp/nest4out
  # exit 0, every lineId prefixed OUTER-
  ```
  One command refuses to proceed over a manifest the other never reads.

- **Corrected conclusion.** **The invoked root wins; the nearest manifest does
  not.** `--project <DIR>` *is* the manifest selector — a `lute.project.yaml`
  closer to the document is not preferred, and with no `--project` none is used.
  `check-project` additionally walks and validates nested manifests, but
  validating a manifest is not the same as letting it govern.

- **What this means for Anseo, plainly.** Anseo's `identity:` block governs only
  when Anseo is compiled *as its own root*. Compiled from `docs/examples`, its
  scenes take the outer manifest's templates. Today that is invisible:
  ```console
  $ lute compile docs/examples/anseo/scenes/wake.lute --project docs/examples      -o /tmp/a-outer.json
  $ lute compile docs/examples/anseo/scenes/wake.lute --project docs/examples/anseo -o /tmp/a-own.json
  $ cmp -s /tmp/a-outer.json /tmp/a-own.json && echo identical
  identical
  ```
  Both give `"lineId": "anseo.s01ep01.vesna_0010"` and `"voiceKey": "vesna-0010"`.
  But that is **coincidence, not resolution**: `docs/examples/lute.project.yaml`
  declares no `identity:` block, and the defaults are exactly
  `{prefix}.{speaker}_{code}` / `{speaker}-{code}` (T1.7) — which is what Anseo's
  block sets. The day `docs/examples` grows an `identity:` block, every Anseo
  artifact built from the outer root silently changes its `lineId`s, and
  `check-project` stays green through it. (The outer-root invocation also prints
  five `lute: E-PROFILE-UNKNOWN` lines for *other* examples' profiles while still
  exiting 0 and emitting the artifact — noted, not pursued; it belongs to
  whichever task touches profiles.)

- **Verdict** — `TOOL-DEFECT`. Not for "invoked root wins" — that is a defensible
  design and it is what the flag's own help says (`compile --project <DIR>`:
  "Project directory (`lute.project.yaml` + `plugins/`) resolving the document's
  activated capability snapshot"; `--all` "Compile EVERY `*.lute` document under
  `--project <dir>`"). The defect is that `check-project` and `compile` disagree
  about whether a nested manifest exists at all — (d) shows one failing the build
  over a file the other does not open. The website states the opposite guarantee
  (`language/frontmatter-and-profiles.md`: "The checker, LSP, and compiler all
  validate the document against the same resolved capability snapshot, so what
  checks clean is exactly what compiles"), and nothing anywhere warns that a
  nested project's manifest is inert under an outer-root build.

- **Correction of record.** The original probe (c) put a distinctive template on
  the **outer** manifest and left the inner one at its defaults, so an
  un-prefixed `lineId` was consistent with *both* hypotheses — and it was read as
  proof of the wrong one. Later tasks must not carry "nearest manifest wins"
  forward; if a task needs Anseo's identity templates to apply, it must compile
  with `--project docs/examples/anseo`.

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

Eleven entries: four *worked well*, three `TOOL-DEFECT`, two `ERGONOMIC` (one of
them the silent-mock case), one `DOC-GAP`, one environment note. Nothing in
Task 1 was inexpressible — every construct the brief asked for compiled, and the
identity chain landed exactly on `anseo.s01ep01.vesna_0010` first try.

The friction is almost entirely *informational*, and the fix pass moved where it
sits. Only **one** entry is a genuine hole in the documentation: T1.7, the
`identity:` block, where the tool knows things (the closed token set, the
derivation of `{prefix}`) it will tell you only after you have guessed wrong. The
other three findings are tools misreporting a world the docs describe correctly —
`lute context` promises the write-surface and ships the vocabulary half of it
while the website carries the forms (T1.6); `E-BAD-ENUM` renders every content
line's speaker as a `::directive` that does not exist (T1.4); `check-project` and
`compile` disagree about whether a nested project's manifest exists (T1.10).

That is a better reading of 0.9.0 than the first pass gave — the language and its
docs are in better shape than the tools that describe them — and a worse one for
anyone trusting a tool's own account of itself. One thing must not be carried
forward from the first pass: T1.10's original "nearest manifest wins" is wrong.
