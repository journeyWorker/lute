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
- **Verdict** — exactly one of the six below. Never invent a verdict or hyphenate a
  hybrid (`AUTHOR-ERROR-adjacent` is not a verdict); if none fits, say so in the entry
  and raise it with the controller, who owns this table.

### Verdicts

| Verdict | Criterion |
|---|---|
| `LANGUAGE-GAP` | The intent cannot be expressed. You changed the story to fit the tool. |
| `ERGONOMIC` | Expressible, but the working form is materially worse than the natural one — more verbose, more indirect, or split across files for no modelling reason. |
| `DOC-GAP` | Expressible and reasonable, but **you had to read Rust source, a proposal, or a test to find it.** The website docs and `lute context` did not get you there. |
| `DOC-WRONG` | The docs are present and **false** — they state a restriction that does not exist, a behaviour that differs, or scope something to the wrong construct. Distinct from `DOC-GAP`, which is silence: silence makes an author search, a false statement makes them stop searching. Rank these above `DOC-GAP` by default; an author who believes a wrong doc never discovers they were lied to. |
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

### T2 — The exits proof

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One beat added
to `scenes/wake.lute`: Vesna decides to take the second pod, and goes back under.

#### T2.1 — an exit written on the line that *is* the departure is accepted, kept, and silently not an exit — TOOL-DEFECT

- **Intent** — Vesna says she is taking the second pod, and returns to cryo. One
  beat: the line, and the character leaving on it.
- **Attempt** — the departure written where the departure happens, on the line
  that *is* the departure:
  ```lute
  @vesna{code="0030" emotion="hollowed" action="go-under"}: If the second pod's intact, I'm taking it.
  ```
  Nothing about this form is speculative. `action` is a documented **line**
  attribute — `language/dialogue-and-cast.md`, "Line attributes": `code`,
  `emotion`, `variant`, **`action`**, `dialogMotion`, `as` — and `go-under` is a
  declared member of the `action` domain, declared in its `exits:`.
- **Result — silence, at every gate I could reach:**
  ```console
  $ lute check docs/examples/anseo/scenes/wake.lute --project docs/examples/anseo
  ok: … (0 warning(s))                                           # exit 0
  $ lute check … --deny-warnings
  ok: … (0 warning(s))                                           # exit 0
  $ lute check-project docs/examples/anseo
  ok: docs/examples/anseo (1 file(s), 0 project-wide warning(s))  # exit 0
  $ lute compile … -o /tmp/t2-probe.json                          # exit 0
  ```
  The artifact keeps the attribute and drops the *exit*:
  ```json
  {"kind":"line","addr":"001-0500","role":"dialogue","speaker":"vesna",
   "text":"If the second pod's intact, I'm taking it.","emotion":"hollowed",
   "action":"go-under","lineId":"anseo.s01ep01.vesna_0030","voiceKey":"vesna-0030"}
  ```
  `[c for c in commands if c.get('exit')]` → `[]`. Vesna never leaves, and she is
  still on stage for the rest of the scene: the only two callers of
  `is_declared_exit` (`inject.rs:193`, `lower.rs:183`) are both on `::auto`'s
  `action`, and the one that removes a character from `StageState.on_stage`
  (`inject.rs:191-197`) is never reached from a content line.

  Scope, stated once so the rest of the entry is precise: it is the **exit**
  reading that is inert in this position, not the attribute. A content line's
  `action` is read, and it emits commands — see T2.2.
- **Is there ANY signal that separates the two positions?** Every surface a
  working author has:
  - `lute check --json` — `"diagnostics": []`, and `resolved.commands_preview`
    renders the whole run as `["::auto", ":vesna", ":vesna", ":vesna"]`. No exit,
    and no way to see one is missing.
  - `lute context` (human) — `auto: character, anchor, action`. Content lines are
    absent from the output entirely (T1.6), so it lists `action` as an `::auto`
    attribute and never mentions the line position. Nothing says either position
    ends a presence.
  - `lute context --json` — **the one place in the toolchain where the fact
    exists.** The `auto` entry carries
    `"semantics": ["reads.onStage","usesAnchor","mayExitCharacter","writes.characterState"]`.
    `mayExitCharacter` is the machine-readable statement that `::auto` is the
    construct that can end a presence. It is dropped from the human rendering of
    the same command, and it appears **nowhere** in
    `packages/website/src/content/docs/` — no hit across the shipped site. The
    only files in the repo that name it are `crates/`, `docs/architecture.md`,
    `docs/plugin-system.md`, and two proposals.
  - `lute trace` and `lute run` — both render every sprite record with no action
    and no exit marker (`<auto>` and `sprite` respectively), so neither preview
    would have shown me the beat was missing.

  Nothing they run tells them.
- **And the checker already warns about this exact shape elsewhere.**
  `check-project docs/examples` emits, twice, over other examples:
  ```
  warning [W-INJECT-CONFLICT] `bianca` is shown with an explicit `anchor="center"` that `auto-anchor-on-show` would otherwise inject
  ```
  A `W-` code whose entire job is "this staging attribute you wrote is not doing
  what you think it is doing". So the precedent exists. So does the information:
  the resolved `action` domain is demonstrably in hand at the content-line check —
  T1.4 has it enumerating all seven members in an `E-BAD-ENUM` on a line's
  `action=` — and `is_declared_exit` is `pub` for exactly this reason. The warning
  is simply not written.
- **Resolution** — staged the departure as its own directive, i.e. the beat
  written as two events instead of one:
  ```lute
  @vesna{code="0030" emotion="hollowed"}: If the second pod's intact, I'm taking it.
  ::auto{character="vesna" action="go-under"}
  ```
- **Verdict** — `TOOL-DEFECT`, and the `DOC-GAP` this was first filed as does not
  survive contact with the pages.

  What the website *does* say, plainly, checked before assigning this:
  `language/directives.md` — "Character staging lives on `::auto` with an action
  id (there is no `::sprite`/`::char`) … a character exit is
  `::auto{action="fade-out-down"}`"; and `language/vocabulary.md` §"Member
  semantics" — the `exits:` members are "the members that end a character's
  presence on stage", and such a member "lowers to a `sprite` record carrying
  `exit: true`". A `sprite` record is what `::auto` lowers to. So the working
  form is one sentence on the shipped site, an author who read `directives.md`
  first would have written the `::auto`, and I did not have to open Rust, a
  proposal, or a test to *find the form*. The `DOC-GAP` bar is not met, and
  claiming it inflated the reading in the same way T1.6's first pass did.

  What fails is a tool, and it fails in the criterion's own words — "a false
  green". The checker holds the resolved `action` domain at the content-line
  check (T1.4 is the proof: `E-BAD-ENUM` enumerates all seven members on a
  line's `action=`), it has `is_declared_exit` exported for the purpose, it has
  a precedent warning of exactly this shape in `W-INJECT-CONFLICT`, and it
  declines to say that a declared-exit member in this position ends nothing.

  It is the protocol's *silence* case in its expensive form. The document is
  green, the string survives into the artifact where a reader will see
  `"action":"go-under"` on the line and assume it means something, and the beat
  is simply absent. One `W-` code closes it — and separately, one sentence in
  `dialogue-and-cast.md` would have kept an author out of the position entirely
  (T2.2).

#### T2.2 — the website never says what a content line's `action` does, and it does something — DOC-GAP

- **Intent** — having found the *exit* reading inert on a line (T2.1), establish
  what `action` in that position actually is. The convenient answer — "nothing" —
  is the one to distrust, so this is checked against the compiler rather than
  assumed from T2.1's silence.
- **Attempt** — read every page that offers the attribute or would carry its
  semantics; then, finding none, read the checker and compile a probe.
- **Result — the documentation, all four surfaces:**
  - `language/dialogue-and-cast.md` offers `action` as one of six line attributes
    and assigns it **no semantics** — "Their *domains* are project vocabulary,
    not grammar". No mention of `::auto`, no cross-reference to `directives.md`.
  - `language/directives.md` attaches "character entrance/exit/pose" to `::auto`.
    So the one plausible meaning of the word is documented on a *different*
    construct, on a page the first one does not point at.
  - `tooling/runtime-contract.md` never lists `action` among a `line` record's
    fields, although the compiler puts it there (`lower.rs:38-49`, `action:
    get("action")`).
  - `posReset` and `auto-pose-reset` — the things a line's `action` actually
    causes — appear **nowhere** in `packages/website/src/content/docs/`. No hit
    across the shipped site.
- **Result — the source. It is read, in two places, and both matter:**
  - `stage_bookkeeping_line` (`crates/lute-check/src/inject.rs:390-397`) writes it
    to the speaker's `SpriteState.pose`;
  - `line_is_stateful` (`inject.rs:405-412`) counts `action` among the four
    sprite-affecting slots, so such a line marks the speaker `dirty`, and a
    *later plain line* from that speaker gets an injected `posReset` under rule
    `auto-pose-reset` (`inject.rs:311-341`).

  This is artifact-visible, and the two scratch scenes differ in exactly one
  attribute. Probe (`/tmp/t2fix/anseo/scenes/pose.lute`):
  ```lute
  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" action="drift"}: A.
  @vesna{code="0020"}: B.
  ```
  ```console
  $ ./target/debug/lute compile /tmp/t2fix/anseo/scenes/pose.lute \
      --project /tmp/t2fix/anseo -o /tmp/t2fix/pose.json          # exit 0
  ```
  ```json
  {"kind":"sprite","addr":"001-0100","character":"vesna","anchor":"port","action":"brace"}
  {"kind":"line","addr":"001-0200","role":"dialogue","speaker":"vesna","text":"A.","action":"drift", …}
  {"kind":"sprite","addr":"001-0300","character":"vesna","posReset":true,
   "provenance":{"injected":true,"by":"auto-pose-reset",
    "reason":"`vesna` had a dirty pose before a plain line; resetting to neutral"}}
  {"kind":"line","addr":"001-0400","role":"dialogue","speaker":"vesna","text":"B.", …}
  ```
  Control (`ctrl.lute`), identical but for dropping `action="drift"` from line
  `0010`: three records, no `posReset`, nothing injected. So a content line's
  `action` emits a command the author did not write, one address later.
  (`emotion` also trips `line_is_stateful`; the `SpriteState.pose` write at
  `inject.rs:395-397` is `action`'s alone, and the control here carries no
  attributes at all, so `action` is the only variable.)
- **Resolution** — none available to an author: this entry *is* the missing
  documentation. `::auto` remains the way to write an exit (T2.1).
- **Verdict** — `DOC-GAP`, and it stands on its own precisely because it is not
  T2.1. T2.1 is about a tool that stays quiet; this is about a page that does not
  exist. What is **not** claimed here is that the exit rule is unstated — it is
  stated, on `directives.md`, and T2.1 turns on that fact. The hole is narrower
  and it is real: the site hands authors an attribute on one page, assigns its
  one plausible meaning to a different construct on another, lists neither in the
  runtime contract's `line` record, and documents the semantics the attribute
  *actually* has — pose state, statefulness, an injected `posReset` — on no page
  at all. I learned them by compiling the probe above and reading `inject.rs`.
  That is the harsh bar met in its literal form: a working author cannot read
  `inject.rs`, and here there is nowhere else to look. Two sentences in
  `dialogue-and-cast.md` close it — what `action` on a line is for, and that a
  character exit is `::auto`.

  **Correction to this entry's first pass**, recorded rather than quietly edited,
  because the protocol's whole value is that its entries are true. The first pass
  asserted that `line.action` is "a pass-through that nothing reads", citing
  `lower.rs:178-198` and `inject.rs:192,432`. All three citations are the
  `::auto` path — `lower.rs:178-198` is the `"auto"` arm of `lower_directive`,
  `inject.rs:192` is `lower_auto`'s exit branch, `inject.rs:432` is
  `is_declared_exit` itself. None of them is the line path, and the assertion
  they were offered for is false.

#### T2.3 — the proof, and its negative control — WORKED WELL

- **Attempt** — brief Steps 2 and 3, the second run as a real control rather than
  a formality: change nothing but the member, `go-under` → `drift`. Both are
  declared members of the same `action` domain; both are equally opaque strings;
  only one is in `exits:`.
- **Result** — the two artifacts differ in exactly one key, at the same address:
  ```json
  go-under: {"kind":"sprite","addr":"001-0600","character":"vesna","action":"go-under","exit":true}
  drift   : {"kind":"sprite","addr":"001-0600","character":"vesna","action":"drift"}
  ```
  Positive: `[c for c in commands if c.get('exit')]` → exactly one record.
  Negative: `[]`. `check --deny-warnings` is clean in both directions, which is
  the point — `drift` is not an error, it is simply not an exit.
- **Verdict** — worked well, and it is the strongest single thing measured so far.
  `exit` is derived from one declared list, in one file, by one function both the
  checker and the compiler call — `is_declared_exit` (`inject.rs:432`), whose only
  two callers are `inject.rs:193` and `lower.rs:183`. It is `Option<bool>`, set to
  `Some(true)` or `None`, so a non-exit omits the key entirely rather than
  serializing `"exit": false`.

  State that guarantee correctly, because this entry's first pass had it exactly
  backwards. `Option<bool>` does **not** let a consumer tell "not an exit" from
  "unset" — those are the *same* absent field, and the encoding collapses the
  distinction rather than preserving it. What the design actually buys is that
  there is nothing left to distinguish: the compiler writes `exit` for precisely
  the declared-exit members and never writes `false`, so absence is total and the
  consumer's rule is one line — **no `exit` key means not an exit.** The negative
  control above is what makes that rule checkable rather than asserted, and it is
  a real guarantee; it is just not the one first claimed.

  Nothing in this vocabulary would have survived the deleted
  `fade-out*`/`exit*`/`hide` heuristic: `go-under` and `step-out` would both have
  been missed, and `drift` would have been correctly ignored only by accident.
  That is the whole argument for the declaration, demonstrated rather than
  asserted.

#### T2.4 — a character exits, keeps speaking, and exits again: `ok`, zero warnings, two `exit: true` — TOOL-DEFECT

- **Intent** — in the committed scene the exit is last, and reading it back
  (T2.5) position is doing all the work of telling a reader that Vesna is gone.
  Find out whether position is doing the *checker's* work too: is "a character
  who left does not speak" a rule the toolchain enforces, or a property my scene
  happens to have?
- **Attempt** — scratch copy of the example so nothing committed moves —
  `cp -R docs/examples/anseo /tmp/t2fix/anseo` — plus one added scene,
  `/tmp/t2fix/anseo/scenes/stage_state.lute`, verbatim:
  ```lute
  ---
  kind: scene
  character: anseo
  season: 1
  episode: 2
  uses: [../vocabulary.schema.yaml]
  ---

  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
  ::auto{character="vesna" action="go-under"}
  @vesna{code="0020" emotion="level"}: So we walk.
  ::auto{character="vesna" action="go-under"}
  ```
  A declared exit; then an ordinary dialogue line from a character who is no
  longer on stage; then a second exit for a character who already left. Three
  things that cannot all be true of one performance.
- **Result — clean, at the strictest setting the CLI offers:**
  ```console
  $ ./target/debug/lute check /tmp/t2fix/anseo/scenes/stage_state.lute \
      --project /tmp/t2fix/anseo --deny-warnings
  ok: /tmp/t2fix/anseo/scenes/stage_state.lute (0 warning(s))       # exit 0
  $ ./target/debug/lute check-project /tmp/t2fix/anseo
  ok: /tmp/t2fix/anseo/scenes/stage_state.lute (0 warning(s))
  ok: /tmp/t2fix/anseo/scenes/wake.lute (0 warning(s))
  ok: /tmp/t2fix/anseo (2 file(s), 0 project-wide warning(s))       # exit 0
  $ ./target/debug/lute compile /tmp/t2fix/anseo/scenes/stage_state.lute \
      --project /tmp/t2fix/anseo -o /tmp/t2fix/stage_state.json     # exit 0
  ```
  Zero diagnostics of any severity, and the artifact carries the contradiction
  straight through:
  ```json
  {"kind":"sprite","addr":"001-0100","character":"vesna","anchor":"port","action":"brace"}
  {"kind":"sprite","addr":"001-0200","character":"vesna","preload":true,"emotion":"clipped", …}
  {"kind":"line","addr":"001-0300","role":"dialogue","speaker":"vesna","text":"Cryo's gone. We don't go back under.", …}
  {"kind":"sprite","addr":"001-0400","character":"vesna","action":"go-under","exit":true}
  {"kind":"line","addr":"001-0500","role":"dialogue","speaker":"vesna","text":"So we walk.", …}
  {"kind":"sprite","addr":"001-0600","character":"vesna","action":"go-under","exit":true}
  ```
  `[c for c in commands if c.get('exit')]` → **two** records. A runtime is told to
  hide the sprite, then to play a line from it, then to hide it again.
- **This is not a missing analysis. The state exists, it is correct, and it is
  read on the very next node.**
  - The checker removes the character on the first declared exit — `lower_auto`'s
    exit branch calls `state.on_stage.remove(&character)` (`inject.rs:191-197`).
    So at line `0020` the reducer already knows Vesna is off stage.
  - It then *consults* that knowledge, for a different purpose, on that exact
    line. `auto-pose-reset`'s guard is
    `!stateful && state.dirty.contains(speaker) && state.on_stage.contains_key(speaker)`
    (`inject.rs:319`). The third conjunct is false; the only consequence is that
    an injection is skipped. Absence is used as "nothing to reset" and never as
    "the author staged something impossible".
  - The second `::auto` never tests presence at all: the exit branch fires on
    `is_declared_exit` alone, and its `remove` is a no-op on an absent key
    (`inject.rs:191-197`).
- **Resolution** — `NONE — nothing to resolve; the probe is the finding.` The
  committed scene is correct by construction, not by verification, and I have no
  way to make the toolchain confirm the difference.
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: nothing is inexpressible. The correct scene and the
    incoherent one are both writable; the story never had to change.
  - Not `ERGONOMIC`: the working form is not more verbose or more indirect, it is
    *unverified*. The cost lands on a scene that is wrong rather than one that is
    awkward, which is a different kind of cost.
  - Not `DOC-GAP`: no page's absence caused this, and no page could fix it. There
    is nothing for `directives.md` to say — "a character who has exited cannot
    speak" is not knowledge an author lacks.
  - Not `AUTHOR-ERROR`: I did not miss a documented rule; I wrote a scene that
    contradicts itself and the tool called it `ok`.

  That leaves `TOOL-DEFECT`, and the criterion fits word for word: the language
  and its docs are fine, and the tool is "lying about its own contract" — this is
  the "false green" the table names explicitly.

  **Weight.** This is the most serious thing T2 found, and it is more serious than
  T2.1, which is the same silence over a shape the checker has no state for.
  Here the checker has the state, has it correct, and reads it one conjunct away
  from the diagnostic. `--deny-warnings` is the strongest promise the CLI makes;
  an author or a CI harness that trusts it ships staging that no runtime can
  perform, with no diagnostic, no warning, and an artifact that looks deliberate.
  The precedent is already in the codebase — `W-INJECT-CONFLICT` (T2.1) exists to
  say "this staging attribute is not doing what you think it is doing", so the
  severity tier and the reporting path are settled. What is missing is a
  `contains_key` on the line arm and one on the exit branch.

#### T2.5 — the finished source cannot tell you which `::auto` is the exit — ERGONOMIC

- **Intent** — read the shot back as an author who did not write it.
- **Attempt** — the committed scene, in full:
  ```lute
  ## Cold Wake
  ::auto{character="vesna" anchor="port" action="brace"}
  @vesna{code="0010" emotion="clipped"}: Cryo's gone. We don't go back under.
  @vesna{code="0020" emotion="level"}: So we walk.
  @vesna{code="0030" emotion="hollowed"}: If the second pod's intact, I'm taking it.
  ::auto{character="vesna" action="go-under"}
  ```
- **Result** — the entrance and the exit are the same construct with the same
  attribute names, and the entire difference between "Vesna is now on stage" and
  "Vesna is gone" is which of `brace` and `go-under` appears in a list in
  `../vocabulary.schema.yaml`. Position is a hint, not a rule: the exit happens to
  be last here, nothing requires that, and nothing checks it — see **T2.4**, where
  a scene that exits, keeps speaking, and exits again is `ok: … (0 warning(s))`
  under `--deny-warnings`. No author-facing surface annotates the difference in
  place: `lute trace` prints both directives as `<auto>`, `lute run` prints both
  records as `sprite`.
- **Resolution** — none; the source stands as written, and the adjacent
  line/directive pair reads acceptably here only because the line carries
  `emotion=` and the directive carries `action=`. Had the beat wanted both, the
  two adjacent lines would be genuinely ambiguous to a reader. The one command
  that helps is `lute doctor`, which prints the resolved semantics on one line:
  `• vocabulary slots declared: emotion, action (exits: step-out/go-under), anchor (default: center), …`
- **Verdict** — `ERGONOMIC`, and scoped to readability alone now that the
  unchecked-staging half of it is T2.4. This is the deliberate 0.9.0 trade and the
  entry is not an argument against it: a declared list beats a name prefix
  precisely *because* `go-under` is unguessable. But the cost is real and it lands
  on the reader rather than the writer — staging semantics are now non-local, and
  the three tools that render a scene for a human (`trace`, `run`, `context`'s
  human mode) each discard the one bit that says a character left. `trace`
  printing `<auto exit>`, or `context` keeping the `semantics` flags its own
  `--json` already carries, would close it without touching the language.

#### T2 summary

Five entries: two `TOOL-DEFECT`, one `DOC-GAP`, one `ERGONOMIC`, one *worked
well*. The mechanism under test is sound — the negative control is clean, the
field is absent rather than false, and the declaration does exactly the work the
heuristic used to guess at. Everything that went wrong is on the *approach* to it
and on the *verification* of it. On the approach: the position that carries the
exit is stated on one page, the position that does not is offered on another with
no semantics at all even though it has them, and no preview tool shows the
difference in the result. On the verification: the checker accepts a declared-exit
member on a content line without a word (T2.1) and accepts a character speaking
after they have left (T2.4), the second while holding the state that refutes it.
An author gets the exit right by having read the correct page first, or by
compiling and diffing the JSON — and gets no help at all in finding out they got
the staging wrong.

### T3 — The shed clock as declared state

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One scene added:
`scenes/cryobank.lute`, `anseo.s01ep02` — the project's first `<branch>`, its first
`::set`, its first `::assert`, and the route ancestor of everything downstream.

The scene carries a design claim: **Lute has no engine clock.** `run.shedPressure` is
the shed schedule, and it advances only where an author wrote `::set`. T3.1 is the
proof; T3.2 is what happens when the author writes one that is quietly wrong.

#### T3.1 — the counter is the natural form, and there is no clock behind it — WORKED WELL

- **Intent** — waking crew costs clock. Cracking a pod draws power the Purser bills
  against the shed schedule; the engineer costs more than the navigator, and leaving
  both under costs nothing. Three choices, two of which move a number.
- **Attempt** — the form I reached for first, unmodified, inside the choice bodies:
  ```lute
  <choice id="wakeToma" label="Wake the engineer">
  ::set{run.shedPressure += 2}
  ```
  Nothing was substituted to make this compile. It is what a counter looks like.
- **Result** — `check-project docs/examples/anseo` → `ok: … (2 file(s), 0 project-wide warning(s))`,
  exit 0, first try. The artifact carries the increments as state-write commands,
  one per arm, at addresses *inside* the arm:
  ```json
  {"kind":"set","addr":"001-0600","path":"run.shedPressure","op":"+=","value":"2","expr":{"lit":2.0}}
  {"kind":"set","addr":"001-1100","path":"run.shedPressure","op":"+=","value":"1","expr":{"lit":1.0}}
  ```
  Both the surface form (`op: "+="`, `value: "2"`) and the parsed form (`expr`) are
  emitted, so a consumer can round-trip the author's text or evaluate the tree.
- **The no-clock claim, with its negative control.** The whole command inventory of
  the compiled scene is `{sprite: 2, line: 5, choice: 1, set: 2, assert: 4, jump: 3}`
  — no tick, no timer, no time-driven kind, and the strings `tick`/`timer`/`clock`/
  `elapsed`/`narrativeTime` appear nowhere in the artifact. Executed against the
  reference runtime with each arm forced:
  ```console
  $ lute run /tmp/t3-cryobank.json --mock ok_wakeToma.yaml      # choose: { whoWakes: wakeToma }
    001-0600  set    run.shedPressure = 2      -> final run.shedPressure = 2
  $ lute run … --mock ok_wakeIlsabet.yaml      -> final run.shedPressure = 1
  $ lute run … --mock ok_wakeNobody.yaml       -> final run.shedPressure = 0
  ```
  The third is the control that matters: the `wakeNobody` arm contains no `::set`, so
  no `set` command exists on that path and the schedule **does not move**. Nothing in
  the engine advances it on its own. The language also refuses to let you invent a
  clock: declaring `run.clock: { type: narrativeTime }` is rejected, and
  `facts-and-datalog.md` states the one narrative-time anchor an author may write is
  `quest.<id>.activatedAt`. So "the schedule advances only because an author wrote
  `::set`" is not a convention this example adopts — it is the only thing available.
- **A rule the scene silently depends on, and it is enforced.** `+=` reads the old
  value first, so a compound assignment needs the path to be already-assigned.
  `run.shedPressure` carries `default: 0`, which is why the bare `+=` is legal.
  Removing the default from `world.schema.yaml` and re-checking:
  ```
  error [E-MAYBE-UNSET] state path `run.shedPressure` may be read before it is set
  (no default, no dominating `::set`, no guard) (dsl §9.4)
  ```
  `state-model.md` states this rule in one sentence and the checker enforces it
  exactly. (Schema restored; the probe was on a scratch copy.)
- **Resolution** — none needed; the first form written is the committed form.
- **Verdict** — worked well. The natural expression *is* the working expression, the
  increment survives to the artifact unmodified, and the design claim the scene
  carries is demonstrable rather than asserted.

#### T3.2 — a `::set` right-hand side is not typed against the path it writes; the runtime then eats it — TOOL-DEFECT

This is T3's most serious finding, and it is the failure mode a counter cannot survive.

- **Intent** — none authorial. It fell out of asking the assignment's question "does
  anything tell you a `::set` target is a state path you declared?" — the answer for
  the *target* is an excellent yes (T3.4). So I asked the same question of the
  *value*, because a number that silently fails to increment is the single worst thing
  that can happen to this scene.
- **Attempt** — three writes to `run.shedPressure`, declared `{ type: number, default: 0 }`:
  ```lute
  ::set{run.shedPressure += "two"}                    # string into a number
  ::set{run.shedPressure = true}                      # bool into a number
  ::set{run.shedPressure += (run.shedPressure > 0) * 3}   # bool arithmetic
  ```
- **Result — all three check clean at the strictest setting:**
  ```console
  $ lute check … --deny-warnings
  ok: /tmp/t3/anseo/scenes/c_strnum.lute  (0 warning(s))    # exit 0
  ok: /tmp/t3/anseo/scenes/c_boolnum.lute (0 warning(s))    # exit 0
  ok: /tmp/t3/anseo/scenes/c_paren.lute   (0 warning(s))    # exit 0
  ```
  All three compile, and the reference runtime — `lute run`, described by its own help
  as "the reference consumer of the runtime contract" — carries them through without a
  word:
  ```console
  $ lute run strnum.json     001-0100  set  run.shedPressure = 0      -> final = 0
  $ lute run boolnum.json    001-0100  set  run.shedPressure = true   -> final = true
  $ lute run paren.json      001-0100  set  run.shedPressure = 0      -> final = 0
  ```
  Exit 0 on all three. `0 += "two"` is silently **0** — the counter does not advance
  and nothing anywhere says so. `= true` is worse: the reference runtime writes a
  boolean into a path the schema declares `number`, and the final-state dump prints
  `run.shedPressure = true`. The `type:` in the schema is not enforced at either end.
- **The asymmetry is the point, and it is inside one construct of each other.** The
  same schema, the same path, the same compiler run:
  - **Relation arguments are typed to the member.** `::assert{awake(nobody)}` →
    `E-FACT-DOMAIN`, naming the entity kind and the argument index (T3.4).
  - **`into=`/`value=` is typed to the path.** `<choice … into="run.shedPressure">`
    without a `value` → `E-INTO-VALUE`: *"`value` is required for `run.shedPressure`
    (only a `bool` path defaults to `true`)"*. That diagnostic can only exist because
    the checker knows this path is a `number` at that moment.
  - **`::set`'s value is typed to nothing.** One construct away, holding the same
    knowledge, it accepts a string.
- **Resolution** — `NONE — nothing to resolve; the committed scene writes integer
  literals and is correct by construction, not by verification.` I have no way to make
  the toolchain confirm the difference.
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: the counter is perfectly expressible, and T3.1 expresses it.
  - Not `ERGONOMIC`: the working form is not more awkward, it is *unverified*.
  - Not `DOC-GAP`: `state-model.md` is unambiguous — state is "a set of **typed** paths
    (`number`, `bool`, `string`, `enum`)", every path "MUST be declared with a `type`".
    No page's absence caused this and no page could fix it.
  - Not `AUTHOR-ERROR`: I did not miss a documented rule; the tool accepted a write the
    documentation says is ill-typed.

  That leaves `TOOL-DEFECT`, in the criterion's own words: the language and its docs
  are fine, and the tool is "lying about its own contract" — a declared type that
  binds nothing. It is also the protocol's *silence* case in the form that costs most.
  A mistyped `emotion` is caught instantly (`E-BAD-ENUM`, T1.4); a mistyped *number*
  reaches the player as a counter that stopped counting, through a green
  `--deny-warnings`, a green compile, and a green reference run. For a work whose
  central mechanic is a schedule that advances, this is the bug you would ship.

#### T3.3 — `::assert` inside a `<choice>` scopes to the arm and reaches a downstream gate — WORKED WELL

Recorded prominently because Task 4's quest gate depends on it, and because a
negative here would have been load-bearing for the next task.

- **Intent** — waking a crew member should be a durable fact about the run, not a
  scene-local flag: Task 4 must be able to ask "is Toma awake, and does he know the
  shed sequence?" from a different document, in a different episode.
- **Attempt** — the assertions written where the waking happens, inside the arm:
  ```lute
  <choice id="wakeToma" label="Wake the engineer">
  ::set{run.shedPressure += 2}
  ::assert{awake(toma)}
  ::assert{knows(toma, shed_sequence)}
  ```
- **Result** — the facts land inside the arm, before its jump to the converge point,
  so they are conditional on the selection rather than unconditional in the scene:
  ```json
  {"kind":"choice","addr":"001-0500","branchId":"whoWakes","recordKey":"scene.choices.whoWakes",
   "options":[{"id":"wakeToma",…,"target":"001-0600"},{"id":"wakeIlsabet",…,"target":"001-1100"},
              {"id":"wakeNobody",…,"target":"001-1600"}],"converge":"001-1800"}
  {"kind":"set",   "addr":"001-0600","path":"run.shedPressure","op":"+=","value":"2"}
  {"kind":"assert","addr":"001-0700","relation":"awake","args":["toma"]}
  {"kind":"assert","addr":"001-0800","relation":"knows","args":["toma","shed_sequence"]}
  {"kind":"line",  "addr":"001-0900",…,"speaker":"toma"}
  {"kind":"jump",  "addr":"001-1000","target":"001-1800"}
  ```
  Four `assert` records total, two per waking arm; the `wakeNobody` arm has none.
- **The full chain to Task 4's gate, verified end to end.** A scratch scene asserts in
  a choice arm, then queries the *derived* relation in a later shot:
  ```lute
  @vesna{code="0030" emotion="level" when="holds(can_halt(toma))"}: Then you can stop the shed.
  @vesna{code="0040" emotion="hollowed" when="!holds(can_halt(toma))"}: Nobody here can stop it.
  ```
  `check --deny-warnings` clean; the guards lower to `match` records carrying the
  compiled predicate:
  ```json
  {"kind":"match","addr":"002-0100","subject":"holds(can_halt(toma))","arms":[{"test":"(holds(can_halt(toma)))",…}]}
  ```
  So `::assert{awake(toma)}` + `::assert{knows(toma, shed_sequence)}` in a choice arm
  → the `world.schema.yaml` rule `can_halt(C) :- awake(C), knows(C, shed_sequence)` →
  a `holds()` guard in a later document. The whole path Task 4 needs is live. Both
  base relations are `tier: run`, so they survive to the next episode, which
  `scene.choices.whoWakes` explicitly does not (`choices-and-hubs.md`: that path
  "clears at episode end").
- **Verdict** — worked well, without qualification. This is the single most important
  thing T3 needed to be true and it was true first try.
- **One documentation wrinkle, filed separately.** The page that enumerates the
  built-in `::`-directives scopes `::assert` / `::retract` to quest documents, which
  is false in exactly the way this entry demonstrates. It is now its own entry —
  **T3.13**, `DOC-WRONG` — because it is a defect in the docs, not in the construct.
  The verdict here is unaffected: the construct itself worked first try.

#### T3.4 — the relational and state-path diagnostics are the best surface measured so far — WORKED WELL

This is the direct answer to "does declaring `entities:` earn its keep?", so it gets
the full transcript. Every probe is one scratch scene, one deliberate mistake.

```
::set{run.shedPresure += 2}
  10:7:  error [E-UNDECLARED] `::set` target `run.shedPresure` is not declared in the
         `state:` schema (dsl §7.3.4) — did you mean `run.shedPressure`?

::assert{knows(toma)}                              # wrong arity
  10:1:  error [E-RELATION-ARITY] relation `knows` expected 2 argument(s), got 1

::assert{awake(nobody)}                            # non-member
  10:1:  error [E-FACT-DOMAIN] `nobody` is not a declared member of entity kind `crew`
         (relation `awake` argument 0, dsl 0.3.0 §3.1)

::assert{awak(toma)}                               # typo'd relation
  10:1:  error [E-RELATION-UNKNOWN] unknown relation `awak` (dsl 0.3.0 §4)

::assert{knows(shed_sequence, toma)}               # arguments transposed
  10:1:  error [E-FACT-DOMAIN] `shed_sequence` is not a declared member of entity kind
         `crew` (relation `knows` argument 0, dsl 0.3.0 §3.1)
  10:1:  error [E-FACT-DOMAIN] `toma` is not a declared member of entity kind `topic`
         (relation `knows` argument 1, dsl 0.3.0 §3.1)

::assert{can_halt(toma)}                           # writing a derived relation
  10:1:  error [E-DERIVED-WRITE] relation `can_halt` is `derive: true`: it is computed
         by `rules:` and MUST NOT be asserted or retracted by content (dsl 0.3.0 §5)

<choice … when="run.vesnaTrst > 0">
  11:32: error [E-UNDECLARED] state path `run.vesnaTrst` is not declared in `state:`
         (dsl §9.4) — did you mean `run.vesnaTrust`?
```

- **Why this is the answer to the maturity question.** Six distinct failure modes, six
  distinct codes, and every one of them names the fix. The transposed-arguments case is
  the standout: a two-argument relation over two different entity kinds catches a swap
  that no amount of naming discipline would, and it reports **per argument index**, so
  the author is told which slot is wrong rather than that the fact "is invalid". This
  is exactly the value that `entities: { crew: …, topic: … }` buys, and it is
  unavailable in any design where `awake` is a string key.
- **Did-you-mean is on every state-path read and write**, including inside a `when=`
  guard and inside a `{{…}}` interpolation, so a typo'd path is a two-second fix
  wherever it appears.
- **The one blemish, small:** relation diagnostics all land at `10:1`, the start of the
  directive, never on the offending argument — including the transposition case, where
  two errors share one span. `::set` and `when=` are column-exact by contrast (`10:7`,
  `11:32`). The message body carries the argument index, so nothing is lost; it is one
  span computation short of perfect.
- **Verdict** — worked well, and it is the strongest counterweight in this log to T3.2.
  The relational layer's arguments are typed to the *member*. The scalar layer's
  `::set` value is typed to *nothing*. Same file, same compiler run, same author.

#### T3.5 — conditional availability works, is guarded against emptiness, and was discoverable — WORKED WELL

- **Intent** — a natural instinct on this beat: "waking the engineer should only be
  offered if Vesna trusts you", and separately "a pod already cracked cannot be cracked
  again". Both are conditional availability of a choice.
- **Attempt** — `when=` on `<choice>`, reached for by analogy with the content-line
  `when=` in `docs/examples/investigation/scenes/confrontation.lute`:
  ```lute
  <choice id="wakeToma" label="Wake the engineer" when="run.vesnaTrust > 0">
  <choice id="a" label="Wake the engineer" when="!holds(awake(toma))">
  ```
- **Result** — both check clean and both reach the artifact with the guard compiled to
  a tree beside its source text:
  ```json
  {"id":"wakeToma","label":"Wake the engineer","when":"run.vesnaTrust > 0",
   "expr":{"op":">","l":{"path":"run.vesnaTrust"},"r":{"lit":0.0}},"target":"001-0300"}
  {"id":"a","label":"Wake the engineer","when":"!holds(awake(toma))","target":"001-0200"}
  ```
  Scalar guards and relational-fact guards both work in choice position, which is what
  the "already cracked" instinct needed.
- **And the obvious way to break it is a hard error.** Guarding every choice:
  ```
  error [E-BRANCH-ALL-GUARDED] `<branch id="bAll">` has no unguarded `<choice>`; every
  choice carries a `when`, so the menu could be empty — a branch must contain at least
  one unguarded choice (dsl §11.1)
  ```
  That rule is why the committed scene's `wakeNobody` is unguarded, and the message
  explains the *reason* rather than just the rule. Neighbouring structural checks are
  equally pointed: `E-CHOICE-DUP` on a repeated choice id within a branch;
  `E-INTO-VALUE` (*"`value` is required for `run.shedPressure` (only a `bool` path
  defaults to `true`)"*) and `E-INTO-TARGET` (*"`into="awake(toma)"` must name a
  `run.<path>` fact"*) when `into=` is misused.
- **`into=`/`value=`: yes, discoverable, and I would have found it.** The assignment
  asks this directly. `language/branch-match-when.md` is the page you reach for when
  you write your first `<branch>` — its title is the construct — and it closes the
  `<branch>` section with: *"Choice mechanics — `when` guards, the `into=` run-record
  sugar, and revisit `<hub>`s — are covered in [Choices & hubs]."* All three of the
  things I wanted, named, in one sentence, with a link, on the page I was already on.
  `choices-and-hubs.md` then gives `into=`/`value=` a worked example and the exact rule
  I would have needed (`value` defaults to `true` only for a `bool` path). This is what
  the T1.6/T1.7 findings were complaining about the *absence* of, and here it is
  present. Worth saying plainly: the language docs did the job the tooling did not.
- **Why the committed scene uses `::assert` and not `into=` anyway** — not a
  workaround, a modelling choice the docs support. `into=` records a *scalar* into a
  `run.*` path; what Anseo needs downstream is a *relation* between a crew member and a
  topic, which `into=` cannot name (`E-INTO-TARGET` says so). The branch already
  records its own selection into `scene.choices.whoWakes` for free — visible in the
  artifact as `recordKey` and in `lute context` as
  `scene.choices.whoWakes: enum [wakeToma, wakeIlsabet, wakeNobody, unset]` — so the
  intra-episode half needs no author action at all.
- **Also checked, and reasonable:** an empty `<choice>` body compiles to a bare jump to
  the converge point while still recording the selection. A "say nothing, but remember
  it" option is expressible without a filler line.
- **Verdict** — worked well. Every conditional-availability idea I had was expressible
  in the form I first reached for.

#### T3.6 — reaching for a guard on `::set` misdirects, and the suggested fix does not parse — TOOL-DEFECT

- **Intent** — "waking the engineer costs more the later you do it": the increment
  should depend on how far the schedule has already advanced.
- **Attempt** — the first form I reached for was a guard on the write itself, by
  analogy with `when=` on lines and on choices, which is the only guard spelling the
  language has shown me:
  ```lute
  ::set{run.shedPressure += 3 when="run.shedPressure > 0"}
  ```
- **Result** — a diagnostic about the wrong thing entirely:
  ```
  10:33: error [E-CEL-PARSE] `=` assigns; comparison is `==` — did you mean
         `3 when=="run.shedPressure > 0"`? (dsl 0.4 §8.1)
  ```
  My `when=` was swallowed into the CEL expression on the right of `+=`, so the parser
  saw a stray `=` and offered the `==` fix it offers for `if (x = 1)`. The real problem
  is that `::set` has no attribute surface at all — it is `::set{path op celExpr}` and
  nothing else. Nothing in the message hints at that.
- **And the suggestion is not merely unhelpful, it is invalid.** Applying it verbatim:
  ```lute
  ::set{run.shedPressure += 3 when=="run.shedPressure > 0"}
  ```
  ```
  10:29: error [E-CEL-PARSE] not a valid condition expression:
         `3 when=="run.shedPressure > 0"` (dsl 0.4 §8.1)
  ```
  The tool proposed a repair and its own next run rejects it. An author who trusts the
  did-you-mean — and T3.4 shows did-you-mean is usually excellent here — is walked one
  step further from the answer.
- **Resolution — the intent is fully expressible, twice over, and neither form is
  worse than what I wanted.** The right-hand side is a complete CEL expression, so the
  scaling cost is a ternary:
  ```lute
  ::set{run.shedPressure += run.shedPressure > 0 ? 3 : 2}     # ok, exit 0
  ```
  and a genuinely guarded write is a `<match>`, which is the construct the language
  designates for state dispatch:
  ```lute
  <match on="run.shedPressure">
  <when test="$ > 0">
  ::set{run.shedPressure += 3}
  </when>
  <otherwise>
  ::set{run.shedPressure += 2}
  </otherwise>
  </match>                                                     # ok, exit 0
  ```
  The committed scene keeps the flat `+= 2` / `+= 1` because the beat wants a fixed
  price per pod, not a rising one — that is an authorial choice made after confirming
  the alternative works, not a substitution made to avoid finding out.
- **Verdict** — `TOOL-DEFECT`, and it is filed for the misdirection, not the missing
  attribute. There is no `LANGUAGE-GAP`: the intent is expressible two ways. There is
  no `DOC-GAP`: `state-model.md` gives the grammar as `::set{path <op> celExpr}` and
  `branch-match-when.md` gives `<match>`; both are on the shipped site. What fails is
  a diagnostic that names the wrong construct and then emits a repair that does not
  compile — the protocol's highest-priority category, "it said X, the real problem was
  Y", with the added cost that following its advice loses you a second round trip.
  A parse failure *inside a `::set` body* has enough context to say the useful thing:
  "`::set` takes no attributes; guard a write with `<match>`/`<when>`."

#### T3.7 — `lute context` says "directives (9)" and omits all four built-in `::`-directives — TOOL-DEFECT

- **Intent** — before writing the first branching scene, ask the tool what may go in
  it. T1.6 already established that `context` ships vocabulary and not grammar, so this
  entry is deliberately *not* that complaint: it is about an item that belongs to the
  vocabulary half, in a list `context` does emit, under a header that counts it.
- **Attempt** — `lute context docs/examples/anseo/scenes/cryobank.lute`, and `--json`.
- **Result** — both renderings list exactly nine directives:
  `auto, bg, camera, cut, end, music, sfx, vfx, video`.
  The four `::`-directives this scene is built on, or that any stateful scene is built
  on, are absent from both: **`::set`, `::assert`, `::retract`, `::use`.**
  `language/directives.md` names them explicitly as directives — its §"Reserved
  directives" opens *"Two `::`-directives are built-in rather than staging
  vocabulary"* — so this is not a category quibble about what the word means. Note
  that `::end` **is** in the list, and `::end` is core control flow, not staging
  vocabulary; so the list is not "staging directives only" either. It is nine of
  thirteen with no rule connecting them and a count that implies completeness.
- **What `context` does get right here, and it is a lot** — recorded so the entry is
  not one-sided. With `world.schema.yaml` in scope it renders the entities, the
  relations *with arity and argument kinds* (`knows/2(crew, topic)`), the derived
  marker (`can_halt/1(crew) [derive]`), the rule text, and the state schema — including
  `scene.choices.whoWakes: enum [wakeToma, wakeIlsabet, wakeNobody, unset]`, the
  reserved path my own `<branch>` had just brought into existence. For the relational
  layer, `context` is genuinely the best surface in the toolchain.
- **Resolution** — wrote the scene from the brief and the website. From `context` alone
  I could not have learned that `::set` or `::assert` exist.
- **Verdict** — `TOOL-DEFECT`, on the same criterion as T1.6 but for a different
  reason, and it is worth keeping the two apart. T1.6's missing items (the content-line
  form, frontmatter, headings, `code`) are *grammar*, which the docs deliberately own —
  `dialogue-and-cast.md` says so from the other side. A directive name is not grammar;
  it is the exact kind of project-resolved vocabulary this output exists to enumerate,
  it sits in a section headed `directives (N)`, and the parenthesised count asserts
  the list is whole. An AI harness pointed at `--json` — which the `--help` text
  invites — will never emit a `::set`, and nothing in the output signals an omission.

#### T3.8 — a single-brace `{run.shedPressure}` in a choice label is silently literal text — AUTHOR-ERROR

- **Intent** — make the price visible on the button: "Wake the engineer (schedule 4)".
  `choices-and-hubs.md` says a label "may interpolate" and gives no syntax on that page,
  so I guessed.
- **Attempt** — the three spellings a working author would try, in the order I tried
  them. Single braces first, because `{…}` is the attribute-block delimiter everywhere
  else in Lute (`@vesna{…}`, `::set{…}`), so it is the most Lute-shaped guess:
  ```lute
  <choice id="a" label="Wake the engineer (schedule {run.shedPressure})">
  <choice id="a" label="Wake the engineer (schedule ${run.shedPressure})">
  <choice id="a" label="Wake the engineer (schedule {{run.shedPressure}})">
  ```
- **Result — all three `ok`, exit 0 under `--deny-warnings`, and only one of them
  means anything:**
  ```json
  {run.shedPressure}    -> "label":"Wake the engineer (schedule {run.shedPressure})"
  ${run.shedPressure}   -> "label":"Wake the engineer (schedule ${run.shedPressure})"
  {{run.shedPressure}}  -> "label":"Wake the engineer (schedule {{run.shedPressure}})",
                           "placeholders":[{"kind":"path","path":"run.shedPressure"}]
  ```
  Two of the three reach the artifact as literal text a player will read off a button.
  Three mutually exclusive syntaxes, one diagnostic between them: none.
- **The checker is not blind here — it is looking one character too narrowly.** Inside
  the *correct* delimiter, a typo is caught with a suggestion:
  ```
  label="… {{run.shedPresure}}"
  11:1: error [E-UNDECLARED] state path `run.shedPresure` is not declared in `state:`
        (dsl §9.4) — did you mean `run.shedPressure`?
  ```
  Inside single braces, the identical typo is silent, because the whole span is text.
  So the resolver, the path table, and the did-you-mean machinery are all present and
  correct at that exact position; they are simply never asked.
- **Resolution** — the committed scene's labels are plain text, which is what the beat
  wanted anyway. The finding is the near-miss, not the label.
- **Verdict** — `AUTHOR-ERROR`. The docs say so plainly and I did not read them before
  guessing: `dialogue-and-cast.md` states the form — "Content `Text` (and a `<choice>`
  label) may embed **`{{…}}`** interpolations" — on the shipped site, not in Rust. Given
  that, `check` treating `{run.shedPressure}` as literal text is the *correct*
  behaviour, not a violated contract: single braces are ordinary prose, Lute never
  claimed them as an interpolation delimiter, and a tool that faithfully reproduces
  characters the language does not reserve is doing its job. The `W-` code I argued for
  below would be a **new lint heuristic**, i.e. a feature request — and a feature that
  does not exist cannot be a tool lying about its own contract. Downgraded from
  `TOOL-DEFECT` on that reasoning.
- **Why it is kept rather than deleted, stated plainly.** The `AUTHOR-ERROR` criterion
  admits an entry only "if the diagnostic pointed somewhere unhelpful", and **that
  clause does not apply here — there was no diagnostic at all.** It is kept under the
  other standing rule instead, *Also record, always → Silence*: I wrote something
  plausible, nothing complained, and it did not do what I meant. All three spellings
  are `ok` under `--deny-warnings` and two of them ship a state path to a player's
  button. That is the entry's whole value, and it is an observation about silence, not
  a claim of defect.
- **The wish, recorded as a wish.** The low-noise rule would be *single braces wrapping
  a string that resolves to a declared state path* — the checker already has the path
  table open at that span (it fires `E-UNDECLARED` with did-you-mean one character
  over), so a `W-` code there would hit essentially nothing else, with
  `W-INJECT-CONFLICT` (T2.1) as precedent. Separately, one sentence on
  `choices-and-hubs.md` linking to the interpolation section would have prevented the
  guess entirely; today that page says "may interpolate" and stops. Neither is a
  finding against 0.9.0 as shipped.

#### T3.9 — a broken state schema is reported as a count with no message, and the obvious way to look closer misparses it as a scene — TOOL-DEFECT

Found while probing whether an author may declare their own clock (T3.1). This scene
is the first in the project to `uses:` `world.schema.yaml`, so every later task's
schema edit lands on this diagnostic.

- **Intent** — declare `run.clock: { type: narrativeTime }` and find out whether the
  language lets an author invent a time axis.
- **Attempt** — one line added to a scratch `world.schema.yaml`, then `lute check` on a
  scene that imports it.
- **Result** — rejected, correctly, and unusably:
  ```
  scenes/g_clock.lute:1:1: error [E-USES-PARSE] schema import
  `/private/tmp/t3/anseo/world.schema.yaml` has parse/frontmatter errors (1 issue(s))
  ```
  A count. Not the issue. The message names the file and the number of problems in it
  and never the problem, and `--json` carries exactly the same single diagnostic with
  nothing nested inside it. A *hard YAML syntax error* in the same file produces the
  byte-identical shape — `(1 issue(s))` — so the author cannot even tell a typo'd type
  name from unbalanced brackets.
- **Every other way to look closer, and what each does:**
  - `lute check-project` — repeats the same count-only line once per importing
    document. In this project that is 32 identical lines saying nothing.
  - `lute doctor <project>` — **exit 0**, and prints
    `✓ content documents: 32 .lute file(s)` plus the vocabulary summary. It reports the
    project healthy while the state schema is broken.
  - `lute context <schema>` — exit 0, emits a surface with `stateSchema (0):`. Zero
    declared paths is exactly what an *empty* schema looks like, so the one output that
    could have shown the damage renders it as absence.
  - `lute check world.schema.yaml` — the natural next move, and the worst outcome. It
    parses the YAML schema **as a scene document**:
    ```
    world.schema.yaml:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is
      missing; every root document must declare `kind: scene` or `kind: quest`
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `character` is missing
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `season` is missing
    world.schema.yaml:1:1: error [E-META-MISSING] required meta key `episode` is missing
    world.schema.yaml:2:3: error [E-UNCLASSIFIED] unrecognized line
    … one E-UNCLASSIFIED per line of the schema
    ```
    This is not a bad error, it is a *wrong* one: it is the same flood for a perfectly
    valid schema file, it never mentions the actual defect, and an author who follows
    its advice adds `kind: scene` to their state schema and destroys it.
  - There is no `lute explain`; `lute --help` lists no subcommand that opens a schema.
- **Resolution** — I recovered only because I had just typed the offending line and
  could bisect it out. An author who edits a schema and comes back an hour later has
  a count and a file.
- **Verdict** — `TOOL-DEFECT`, and the strongest sub-case is the misdirect, which the
  protocol ranks above almost everything: the one command whose name promises to check
  a file tells you your state schema is missing `kind:`, `character:`, `season:` and
  `episode:`. The information plainly exists — something produced the count `1`, so the
  underlying issue list is in hand and is discarded at the reporting boundary. That is
  the `TOOL-DEFECT` criterion word for word: "the information exists, but the tool that
  promised to hand it to you did not." Related to T1.9 and T1.10 in kind — this
  toolchain's project-level diagnostics repeatedly lose either their location (T1.9's
  `0:0`, T1.10's manifest with no path) or, here, their body.
- **Confirmed in passing:** `type: narrativeTime` is not author-declarable, which is
  the T3.1 claim's other half. The rule is stated on `facts-and-datalog.md`
  (`E-TEMPORAL-ARG`); the diagnostic that enforces it is the one above.

#### T3.10 — an unknown key in a mock file is silently ignored by both `trace` and `run` — TOOL-DEFECT

- **Intent** — drive each arm of the branch to prove the counter moves (T3.1's
  verification). Writing the mock from memory rather than the page, I guessed the key.
- **Attempt** —
  ```yaml
  state:
    run.shedPressure: 0
  selections:
    whoWakes: wakeToma
  ```
- **Result** — `lute run artifact.json --mock that.yaml` accepts the file, makes no
  selection, and stops:
  ```
  001-0500  choice [whoWakes] -> (none)
  -- final state --
    run.shedPressure = 0
    scene.choices.whoWakes = unset
  run incomplete
  ```
  Exit 3. Nothing says the mock contained a key it did not understand; "incomplete" is
  also what you get from a mock that is simply missing the selection, so the two are
  indistinguishable. I initially read this as "`--mock` does not carry selections to
  `run` at all" — which would have been a much bigger and entirely false finding — and
  only caught it by reading `tooling/tracing.md`, where the key is `choose:`.
  With `choose: { whoWakes: wakeToma }` it works perfectly and exits 0 (T3.1).
  `lute trace --mock` with the same bogus keys (`selections:`, `chose:`) is exit **0**
  and walks the scene as if no mock had been supplied.
- **Resolution** — used the documented key. The mock format is on the page, correctly,
  with all five surfaces in one YAML block.
- **Verdict** — `TOOL-DEFECT`. Not `AUTHOR-ERROR` — or rather, it began as one, and it
  is recorded because the diagnostic pointed nowhere, which the table says is exactly
  when to record one. The contract being broken is explicit: `trace --help` says it
  "refuses (exit 1) a document with check errors OR **invalid mocks**", and
  `tracing.md` enumerates six `E-TRACE-MOCK-*`/`E-TRACE-*` refusals — every one of them
  for a bad *value* (undeclared path, wrong literal type, unknown relation, ineligible
  choice). An unrecognised *key* is not among them and is not refused. A mock whose
  selection key is misspelled is an invalid mock; both tools read it, silently discard
  half of it, and report a result. `trace`'s header does whisper the truth —
  `(seeds: 1 paths, 0 facts; 0 selections)` — which is the only tell in either tool,
  and it is a count you have to already suspect. This compounds T1.9: mocks are the one
  part of a Lute project with no checker over them, and now with no key validation
  inside the tools that do read them.

#### T3.11 — the route-ancestor chain is verified project-wide, with did-you-mean — WORKED WELL

- **Intent** — this scene is the ancestor of everything downstream, so `after:` is
  load-bearing in a way it was not for `wake.lute`. Find out whether a wrong scene key
  is caught, or whether the eleven-scene graph can quietly come apart.
- **Attempt** — `after: 'visited("anseo.s01ep01")'` as committed, plus two deliberate
  breakages: a wrong episode (`anseo.s01ep99`) and a misspelled character
  (`anso.s01ep01`).
- **Result** — both caught at project level, both with a suggestion:
  ```
  scenes/a_typo.lute:7:1: error [E-CONN-UNKNOWN-NODE] unknown node: no scene resolves to
    key `anseo.s01ep99` (`visited`, dsl §2.3/§4.1) — did you mean `anseo.s01ep01`?
  scenes/a_bad.lute:7:1:  error [E-CONN-UNKNOWN-NODE] unknown node: no scene resolves to
    key `anso.s01ep01`  (`visited`, dsl §2.3/§4.1) — did you mean `anseo.s01ep01`?
  ```
  And `lute scenario` reports the resulting graph directly, which is the readback the
  eleven-scene structure will need:
  ```console
  $ lute scenario docs/examples/anseo
    topological layers:
      layer 0: scene(anseo.s01ep01)
      layer 1: scene(anseo.s01ep02)
    edges (prerequisite -> dependent) [atom kind(s)]:
      scene(anseo.s01ep01) -> scene(anseo.s01ep02) [visited]
  ```
- **Verdict** — worked well. `lute scenario` is the tool T2.5 wished for and did not
  have: a rendering that shows the structural fact rather than making you infer it.
- **One caveat later tasks must carry**, not a defect — the subcommand help states it:
  the per-file `lute check` on a scene whose `after:` names a nonexistent scene prints
  `ok: … (0 warning(s))`, exit 0. `E-CONN-UNKNOWN-NODE` is a *project-wide* diagnostic
  and only `check-project` computes it. Checking one file is not enough to know the
  route is intact.

#### T3.12 — `trace` and `run` render branching honestly — WORKED WELL

- **Attempt** — read the committed scene back through both preview tools.
- **Result** — `lute trace` shows the construct, the eligible set, the winning arm, and
  every effect inside it:
  ```
  <branch whoWakes>   eligible: wakeToma, wakeIlsabet, wakeNobody   -> wakeToma (auto)
    ::set  run.shedPressure = 2
    ::assert  awake(toma)
    ::assert  knows(toma, shed_sequence)
    @toma  How long have I been under?
  trace complete: 1 decision; choices 1/3 (whoWakes)
  ```
  `lute run` does the same over the artifact, with addresses and a final-state dump.
- **Verdict** — worked well, and it is the direct contrast to T2.5, where the same two
  tools discarded the one bit that said a character had left. For branching, state, and
  facts they discard nothing: eligibility, selection, both effect kinds, and the
  coverage summary are all there.
- **One readback nuance, noted rather than complained about:** trace renders `+= 2` as
  `::set run.shedPressure = 2`, i.e. the resolved post-value, not the delta. Seeded
  with `--state run.shedPressure=7` the same line prints `= 9`. That is arguably the
  more useful number — it is the state after — but for a scene whose subject is *how
  much each choice costs*, the price the author wrote is the thing that is no longer on
  screen. `= 9 (+= 2)` would carry both.

#### T3.13 — `directives.md` scopes `::assert`/`::retract` to "Quest documents"; they work in scenes — DOC-WRONG

Split out of T3.3, where it was found. T3.3 records that the construct worked; this
records that the page telling you whether you may reach for it is false.

- **Intent** — before writing per-arm facts, ask the docs the prior question: may a
  *scene* assert a fact at all, or is fact mutation reserved to quest documents? The
  natural place to look is the canonical enumeration of built-in `::`-directives.
- **Attempt** — read `packages/website/src/content/docs/language/directives.md`
  §"Reserved directives". Lines 124–127, verbatim:
  > Two `::`-directives are built-in rather than staging vocabulary: `::set` writes
  > declared state (see [State model](/state/state-model/)) and `::use` expands a
  > reusable content component (see
  > [Components & extends](/language/components-and-extends/)). **Quest documents
  > additionally use `::assert` / `::retract` to mutate facts** (see
  > [Facts & Datalog](/state/facts-and-datalog/)).

  The false clause is the third sentence, `directives.md:126–127`.
- **Result — false as written, and this task depends on it being false.**
  `docs/examples/anseo/scenes/cryobank.lute` is `kind: scene` (line 2), and four
  `::assert` directives sit inside `<choice>` bodies (lines 18, 19, 24, 25). The
  checker accepts them without qualification —
  `ok: docs/examples/anseo/scenes/cryobank.lute (0 warning(s))`, and
  `lute check-project docs/examples` exits 0 — they lower to real `assert` records
  (`{"kind":"assert","addr":"001-0700","relation":"awake","args":["toma"]}`, T3.3),
  and the facts they write reach a `holds()` guard in a later document. Read plainly,
  the page says the construct is not for the document kind in which it demonstrably
  works. No restriction of the stated shape exists.
- **The docs contradict each other, and that is worse, not better.**
  `packages/website/src/content/docs/state/facts-and-datalog.md:25` states the
  unscoped truth: "Content writes **deltas** with the leaf directives `::assert` and
  `::retract`". So the right answer *is* on the shipped site — which is precisely why
  this is not `DOC-GAP`: nothing is silent, I read no Rust, no proposal, no test. But
  an author asking "which `::`-directives exist and where may I use them?" lands on
  `directives.md` first, because that is the page named after the question. Being told
  the construct belongs to another document kind is a *terminating* answer: they stop
  looking, and never reach the facts page that would have corrected them. A second
  page holding the truth only helps the author who keeps searching, and a false
  statement is exactly the thing that stops them searching.
- **Resolution** — the asserts were written in the scene anyway, and worked (T3.3).
  Resolution for the *doc*: the clause should read that content documents generally —
  scenes included — use `::assert` / `::retract`, or simply drop "Quest documents" and
  say "Content additionally uses", matching `facts-and-datalog.md`.
- **Verdict** — `DOC-WRONG`. Present and false: it scopes a construct to the wrong
  document kind. Ranked above a `DOC-GAP` per the table — an author who believes it
  never discovers they were lied to, and in this project's case would have hand-rolled
  a state flag for something the language already does, losing the Datalog derivation
  (`can_halt(C) :- awake(C), knows(C, shed_sequence)`) that Task 4's gate depends on.

#### T3 summary

Thirteen entries: six *worked well*, five `TOOL-DEFECT`, one `DOC-WRONG`, one
`AUTHOR-ERROR`, no `LANGUAGE-GAP`, no `DOC-GAP`. Nothing this scene wanted was
inexpressible, and — the part that matters for the authoring rule — nothing was
substituted. The counter, the branch, the
per-arm facts, the conditional availability, and the scaling cost were each written in
the form I first reached for, and each of them worked.

**The design claim holds and is now demonstrated, not asserted.** The compiled scene
contains `{sprite: 2, line: 5, choice: 1, set: 2, assert: 4, jump: 3}` and no
time-driven command of any kind; the reference runtime moves `run.shedPressure` to 2,
to 1, or not at all, strictly according to which `::set` an author placed in which arm;
and the language refuses to let an author declare a clock of their own. There is no
engine clock.

**The split in the findings is sharp and it is not about the language.** Everything
*declared* is checked superbly: relation arity, per-argument entity domains, derived-
relation writes, undeclared and misspelled state paths in every position including
inside `{{…}}`, `E-BRANCH-ALL-GUARDED`, `E-INTO-VALUE`/`E-INTO-TARGET`,
`E-MAYBE-UNSET`, `E-CONN-UNKNOWN-NODE`. T3.4 is the answer to whether `entities:` earns
its keep — a transposed `knows(shed_sequence, toma)` is caught per argument index, and
no string-keyed design could do that.

Set against it, **T3.2 is the finding to act on**: the declared `type:` of a state path
constrains `into=`/`value=` and constrains nothing about `::set`. `::set{run.shedPressure += "two"}`
on a `number` path is `ok` under `--deny-warnings`, compiles, and is silently evaluated
to `0` by the reference runtime; `::set{run.shedPressure = true}` writes a boolean into
it. For a work whose central mechanic is a schedule that advances, that is a counter
that stops counting with every gate green.

The remaining four defects are the same shape T1 and T2 found, in new places: tools
that lose information they hold. `context` omits four directives from a list that
counts itself (T3.7); `E-CEL-PARSE` names the wrong construct and proposes a repair
that does not parse (T3.6); a broken state schema is reported as an integer while the
command you would run next misparses the schema as a scene (T3.9); a mock key typo is
discarded by both tools that read mocks (T3.10). Two of the five report a count where
the content was in hand — *say what you found, not how much of it there was* (T3.7,
T3.9) — and two more discard information silently (T3.2's untyped `::set` right-hand
side, T3.10's mock key). T3.9 is the one to fix first, because it is the only one
where the tool's advice actively damages the file.

Outside that shape sit the two reclassified entries. **T3.13 is the finding a reader
of these logs is most likely to hit themselves**: `directives.md` tells authors that
`::assert` / `::retract` are for quest documents, this scene uses them in a `<choice>`
body, and the checker is perfectly happy. One clause, one page, and it is the page
named after the question. T3.8, by contrast, is now an `AUTHOR-ERROR` kept only for
its silence: the shipped docs specify `{{…}}` plainly and single braces are legitimate
prose, so `check` reading them as text is correct behaviour, not a defect.

### T4 — The relational quest gate

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One quest added:
`quests/hold-the-spine.lute`, the project's first `kind: quest` document, gating on the
derived relation `can_halt` that T3.3 proved reachable from a `<choice>` arm two
documents away.

This is the first task to exercise the layer Lute's static-analysis claims are
strongest about, and the reading splits cleanly. **Everything the checker computes
about a relational gate is excellent, and it computes it project-wide.** Everything
that *reports* that computation to an author — one attribute slot, one preview tool,
one reachability verdict, one line of the runtime contract — is wrong, absent, or
contradicts a sibling.

#### T4.1 — every shape a real quest wants was in the language, in the first form reached for — WORKED WELL

- **Intent** — written before the brief's skeleton was typed. The shed is walking down
  the spine toward the infirmary. Vesna wants it stopped, and stopping it needs a hand
  at the coupling belonging to someone awake who knows the sequence. That beat wants
  four things, and only the first is in the brief:
  1. reach the coupling;
  2. **cut** it — an objective that is not offered until the first one is done;
  3. **optionally** pull the manifest from the coupling locker on the way — it does not
     gate the halt, but it matters later;
  4. a way to **fail that is not the inverse of succeeding** — the shed arrives at the
     infirmary first, whatever the crew were doing at the time.
- **Attempt** — all four, written at once, no hedging, against a scratch copy
  (`/tmp/t4/anseo`) with one extra scalar (`run.couplingCut`) so objective 2 had
  something real to read:
  ```lute
  <quest id="holdTheSpine" title="Hold the Spine" start="holds(can_halt(toma))" fail="run.shedPressure >= 5">
  <objective id="reachToma" title="Reach the spine coupling" done="run.shedPressure >= 1"/>
  <objective id="cutCoupling" title="Cut the coupling" done="run.couplingCut" when="quest.holdTheSpine.objectives.reachToma.done"/>
  <objective id="pullManifest" title="Pull the manifest from the locker" done="holds(found(toma))" optional/>
  <on event="questComplete">
  ::set{run.vesnaTrust += 1}
  @narrator: The shed halted, one module short of the infirmary.
  </on>
  <on event="questFailed">
  @narrator: The shed reached the infirmary bulkhead and kept walking.
  </on>
  </quest>
  ```
- **Result** — the *grammar* took all four without a murmur. The only diagnostic in the
  run was semantic, about the story rather than the shape (T4.2). Specifically:
  - **Sequencing is a reserved-path read**, and it composes: an objective may gate its
    own visibility on another objective's completion by reading
    `quest.<id>.objectives.<oid>.done`, a path the compiler declares for you (it is in
    the artifact's `state` table with `"provenance": "quest:holdTheSpine"`). No new
    construct, no author-declared mirror flag.
  - **`optional`** is a bare attribute and excludes the objective from derived
    completion, exactly as `quests-and-scenes.md` says.
  - **`fail=` is a sibling of `start=`, over anything CEL can say** — so an independent
    failure clock is one attribute, and `<on event="questFailed">` reacts to it. The
    failure condition genuinely does not have to mention the success condition.
- **The one semantic caveat, and it is documented and correct.** `when=` on an
  `<objective>` gates "visibility/tracking, not the completion obligation"
  (`quests-and-scenes.md`). So `cutCoupling` is *hidden* until `reachToma` is done but
  still *required* for the quest to complete — which is what "becomes available after"
  should mean. Had I wanted "and skippable if never offered", that is `optional`, and
  the two compose.
- **Resolution** — the committed file is the brief's single-objective form. That is an
  authorial choice made *after* confirming the richer form works, in the same sense as
  T3.6's flat `+= 2`: this quest is the prologue's one-line goal machine, and Task 9's
  five siblings are where the sequencing and the optional arm belong. Nothing was
  substituted to avoid finding out.
- **Verdict** — worked well. Four independent quest-design instincts, four constructs,
  zero workarounds, and the sequencing one did not even need a new idea — it falls out
  of the reserved state the quest already declares.

#### T4.2 — the checker proves a fact gate dead *across documents*, and says which relation — WORKED WELL

This is the strongest single thing T4 measured and it deserves the transcript.

- **Intent** — none authorial; it fell out of T4.1. `pullManifest` gated on
  `holds(found(toma))`, and `found` is declared in `world.schema.yaml` but asserted by
  no document in the project.
- **Result** — a hard, project-wide error naming the relation and the reason:
  ```
  quests/hold-the-spine.lute:11:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate
  `holds(found(toma))` queries relation(s) `found`, which is unreachable under your
  declared routes: no `facts:` seed, no `reserved` tier, and no rule closure over
  already-producible relations can ever populate it, so the objective can never
  complete on any run (dsl 0.4.0 §4.2/§5.3)
  ```
  Change one relation to one the story *does* produce — `holds(knows(toma, manifest))`,
  where `knows` is asserted in `scenes/cryobank.lute`'s choice arms — and the error
  becomes `W-UNPROVEN-RELATIONAL`, exit 0.
- **And the difference really is the other document.** The negative control, run in the
  scratch copy: delete the two `::assert{knows(…)}` lines from `cryobank.lute` and
  re-check, changing nothing in the quest.
  ```
  quests/hold-the-spine.lute:11:1: error [E-OBJECTIVE-UNSATISFIABLE] `done` predicate
  `holds(knows(toma, manifest))` queries relation(s) `knows`, which is unreachable …
  ```
  Warning → error, from an edit in a file the quest does not name and cannot see. The
  producibility analysis is genuinely project-wide, and it is closed over the rule set:
  `can_halt` is never asserted anywhere either, and it is judged producible because
  `can_halt(C) :- awake(C), knows(C, shed_sequence)` closes over two relations that are.
  When the objective is required, the message even carries the consequence up a level —
  "the objective — and, being required, the quest — can never complete".
- **Verdict** — worked well, without qualification. A goal machine in its own file,
  gated on a Datalog head derived from base facts asserted inside a `<choice>` arm of a
  different episode, and the checker still knows whether the gate can ever open. This is
  the payoff the declared relational layer is *for*, and no string-keyed flag design
  could compute it.

#### T4.3 — Step 3: the gate is typed, and `vesna` passing is the interesting half — WORKED WELL

- **Intent** — the assignment's central proof: a typo in a quest gate is a check-time
  error, and that is what a closed entity domain buys.
- **Attempt and result** — three runs against the committed file, exit codes exact:
  ```console
  # A — a name that is not a crew member
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(nobody))"
  quests/hold-the-spine.lute:8:56: error [E-FACT-DOMAIN] `nobody` is not a declared
    member of entity kind `crew` (relation `can_halt` argument 0, dsl 0.3.0 §3.1)
  failed: docs/examples/anseo (3 file(s), …)                                # exit 1

  # B — a crew member who cannot, in this story, ever halt the shed
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(vesna))"
  ok: docs/examples/anseo (3 file(s), 1 project-wide warning(s))            # exit 0

  # C — restored
  $ lute check-project docs/examples/anseo      # start="holds(can_halt(toma))"
  ok: docs/examples/anseo (3 file(s), 1 project-wide warning(s))            # exit 0
  ```
  The error is column-exact on the attribute value (`8:56`), names the entity kind, and
  reports the **argument index** — the T3.4 shape, now confirmed in quest-attribute
  position and not just in `::assert`.
- **B is the entry, not A.** `awake(vesna)` and `knows(vesna, shed_sequence)` are
  asserted **nowhere** in the project — the `wakeToma` arm wakes Toma, the
  `wakeIlsabet` arm wakes Ilsabet, and no arm wakes Vesna. So `can_halt(vesna)` cannot
  hold on any run, and the checker accepts it. That is the correct behaviour and it is
  the whole point: the checker validates the query's **shape** and its arguments'
  **domain membership**, and declines to claim anything about runtime truth.
- **How far "declines to claim" goes, measured rather than assumed.** Diff the two
  green runs with the argument name normalised away:
  ```console
  $ diff <(sed 's/vesna/ARG/' out-vesna.txt) <(sed 's/toma/ARG/' out-toma.txt)
  # (no output — identical)
  ```
  A gate the story can open and a gate the story can never open produce **byte-identical
  diagnostics**. The analysis is relation-level, not ground-fact-level: it proved
  `can_halt` producible (T4.2) and stops there. That is a real and stated boundary, not
  a bug — but it is the precise size of the guarantee, and it is worth knowing that
  `E-FACT-DOMAIN` catches `nobody` and nothing catches `vesna`.
- **Verdict** — worked well. Both halves of the brief's claim hold, and the second half
  is sharper than the brief puts it: what the closed domain buys is not "the gate is
  right", it is "the gate is *askable*". Every misspelling, every wrong entity kind,
  every wrong arity is a build break (T4.6); every well-formed question is accepted and
  honestly labelled unproven.

#### T4.4 — `W-UNPROVEN-RELATIONAL` names one tool, and that tool cannot do the job it is named for — TOOL-DEFECT

The assignment asks whether this warning is actionable or a shrug the author learns to
ignore. It is neither, and the real answer is worse than a shrug.

- **The warning, in full:**
  ```
  warning [W-UNPROVEN-RELATIONAL] `start="holds(can_halt(toma))"` is gated by a
  relational fact query over producible relation(s) `can_halt`; static reachability
  analysis (dsl 0.6.1 §2) neither proves nor refutes it. Verify with `lute trace`
  seeds or human review
  ```
  As prose this is close to a model diagnostic: it quotes the offending attribute, names
  the relation, cites the clause, states the limit precisely ("neither proves nor
  refutes"), and — unusually for a `W-` code — **names the remedy**. It is not a shrug.
  It is a referral.
- **Following the referral.** `lute trace` on the quest is genuinely good at the first
  step — it stops at the gate and hands you the exact flag:
  ```console
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo
  trace incomplete: 1 unresolved atom (exit 3)
    unresolved: quest `holds(can_halt(toma))` (holdTheSpine quest) — supply --fact "can_halt(toma)" as a mock
  ```
  Then it comes apart, twice.
- **(a) The rules do not fire on seeds, so the chain under test cannot be exercised.**
  Seeding the two base facts the story actually asserts changes nothing:
  ```console
  $ lute trace … --fact "awake(toma)" --fact "knows(toma, shed_sequence)"
  trace incomplete: 1 unresolved atom (exit 3)
    unresolved: quest `holds(can_halt(toma))` … — supply --fact "can_halt(toma)" as a mock
  ```
  This is documented, in a parenthetical — `tracing.md`: a `--fact` is "a *supplied
  answer*, so it may name a `derive:`/`reserved:` relation" — so it is design, not
  defect. But the consequence is that the only verification route the warning offers
  requires you to **assert the conclusion**, and the rule
  `can_halt(C) :- awake(C), knows(C, shed_sequence)` — the thing the whole quest rests
  on, and the only part of the chain a human could plausibly get wrong — is never
  evaluated by any command an author can run.
- **(b) And when you do supply the conclusion, trace tells you it proves nothing.**
  ```console
  $ lute trace … --fact "can_halt(toma)"
  note: W-TRACE-MOCK-UNPRODUCIBLE — mock fact over relation `can_halt` is not producible
  (no `facts:` seed, no reachable `::assert`, not `reserved`) — the supplied answer can
  never arise from authored producers, so a complete walk seeded with it proves nothing
  about reachable play (§4)
    <quest holdTheSpine>   -> active (holds(can_halt(toma)))
  trace complete: 4 decisions                                              # exit 0
  ```
  The referral closes the loop back onto itself: `check-project` says "verify with
  `lute trace` seeds", and `lute trace` says the seed proves nothing.
- **(c) The two tools contradict each other about the same word, and trace is the one
  that is wrong.** `W-TRACE-MOCK-UNPRODUCIBLE` asserts `can_halt` "is not producible
  (no reachable `::assert`)". `check-project`, in the same project, with the same
  `--project` root, calls `can_halt` a "producible relation" *in the very warning that
  sent me here* — and T4.2 proves that judgement is real, cross-document, and rule-closed.
  The disagreement is scope, and it is isolable in two commands:
  ```console
  # the document that CONTAINS ::assert{awake(toma)} — no warning
  $ lute trace scenes/cryobank.lute --project docs/examples/anseo --fact "awake(toma)"
  trace: … (seeds: 0 paths, 1 facts; 0 selections)
  ## Shot 1. …

  # a different document in the SAME project, same fact — warning
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo --fact "awake(toma)"
  note: W-TRACE-MOCK-UNPRODUCIBLE — mock fact over relation `awake` is not producible …
  ```
  `trace`'s `producible()` is **document-local**; `check-project`'s is **project-wide**.
  Both print the same three-clause justification, so nothing in either output hints that
  they are answering different questions.
- **Resolution** — `NONE — nothing to resolve; the committed gate is correct by T3.3's
  end-to-end verification, which was done by reading the compiled artifact, not by any
  command that claims to verify gates.`
- **Verdict** — `TOOL-DEFECT`, taking the table in order.
  - Not `LANGUAGE-GAP`: the gate is expressible and expressed.
  - Not `ERGONOMIC`: the working form is not more awkward; the *verification* of it is
    circular.
  - Not `DOC-GAP`: `tracing.md` documents the supplied-answer semantics and documents
    `W-TRACE-MOCK-UNPRODUCIBLE`. I read no Rust to establish any of this.
  - Not `DOC-WRONG` — although it is close, and worth saying why not. `tracing.md:58`
    glosses the warning as firing when "no authored producer can ever assert" the
    relation, which is the *project-wide* meaning and is false of `awake` here. But the
    page is describing what the code is plainly meant to do; the code is what deviates.
  - Not `AUTHOR-ERROR`: I followed the diagnostic's own instruction.

  That leaves `TOOL-DEFECT` on the criterion's own words — a tool "lying about its own
  contract". Two separate lies, and they compound: a warning that refers you to a tool,
  and a tool that answers the referral with a false claim about your project. **On the
  assignment's question:** the warning is *not* a noise floor that trains people to
  ignore warnings — it fires on exactly the correct usages, but it fires with a specific,
  honest, quotable statement of an analysis boundary, which is the right thing for a
  checker to do when it cannot decide. Five such warnings already sit on other examples
  and `check-project docs/examples` still exits 0 with all six. What trains people to
  ignore it is not the warning; it is that discharging it is impossible, so the only
  available response *is* to ignore it.

#### T4.5 — a `start=` gate on an unproducible relation is silent, and `scenario reach` calls the quest Reachable — TOOL-DEFECT

T4's most serious finding, and a direct consequence of T4.2 having been done so well one
attribute over.

- **Intent** — none authorial. T4.2 established that `done="holds(found(toma))"` is a
  build-breaking error because nothing in the project can ever assert `found`. The
  obvious next question is whether the same predicate is caught in the slot that decides
  whether the quest ever *starts*.
- **Attempt** — one attribute changed on the otherwise-committed quest, scratch copy:
  ```lute
  <quest id="holdTheSpine" title="Hold the Spine" start="holds(found(toma))">
  ```
- **Result — total silence, and it is quieter than the correct version:**
  ```console
  $ lute check-project /tmp/t4/anseo
  ok: /tmp/t4/anseo/quests/hold-the-spine.lute (0 warning(s))
  ok: /tmp/t4/anseo (3 file(s), 0 project-wide warning(s))                  # exit 0
  ```
  Zero diagnostics of any severity. Note the count: the **correct** gate
  (`holds(can_halt(toma))`) yields one project-wide warning; the gate that can never open
  yields none. The louder signal is the working code.
- **The machinery exists, is wired to this exact slot, and has the diagnostic class
  already.** Three facts from the same command:
  1. `start=` *does* run producibility — that is what emits `W-UNPROVEN-RELATIONAL` when
     the relation is producible. There is simply no "not producible" branch.
  2. `E-QUEST-UNREACHABLE` exists and fires on this attribute:
     ```
     8:1: error [E-QUEST-UNREACHABLE] quest can never complete: `start` decides false —
          the quest never activates (dsl 0.4 §5.3)
     ```
     on both `start="false"` and `start="1 > 2"`.
  3. `E-OBJECTIVE-UNSATISFIABLE` and `E-QUEST-UNREACHABLE` cite **the same spec clause**,
     `dsl 0.4 §5.3`, and the objective one already escalates to the quest ("the
     objective — and, being required, the quest — can never complete").
  So the analysis, the slot, the diagnostic class, and the spec clause are all present.
  One wire is missing.
- **And it is not merely a missing diagnostic — a tool positively asserts the wrong
  answer.** `lute scenario … reach` consults the quest lifecycle, provably:
  ```console
  $ lute scenario /tmp/t4/anseo reach quest:holdTheSpine       # start="false"
    verdict: Unreachable — quest lifecycle proves this quest can never complete
             (E-QUEST-UNREACHABLE), under your declared routes.

  $ lute scenario /tmp/t4/anseo reach quest:holdTheSpine       # start="holds(found(toma))"
    verdict: Reachable — a plain quest with no declared `after` prerequisite,
             reachable by default quest lifecycle under your declared routes.
  ```
  Same tool, same question, same *kind* of dead quest — and for the relational one it
  prints **Reachable**. This is not `scenario` being honestly graph-only: it reaches into
  the lifecycle verdict for the scalar case and gets a right answer, then reports a wrong
  one for the relational case because the verdict it is reading was never computed.
- **Resolution** — `NONE — nothing to resolve; the probe is the finding. The committed
  quest gates on a producible relation, which is correct by T4.2's analysis, not by
  anything the `start=` slot checked.`
- **Verdict** — `TOOL-DEFECT`, and it is the "false green" the table names, in its
  compound form. Not `LANGUAGE-GAP` (nothing inexpressible), not `ERGONOMIC` (the form is
  fine, the verification is absent), not `DOC-GAP` (no page's absence causes it and none
  could fix it — `scene-graph.md` and `quests-and-scenes.md` both describe the intended
  behaviour correctly), not `AUTHOR-ERROR` (I broke no documented rule; the tool called a
  dead quest live). A quest whose `start` predicate can never become true is a quest that
  is never playable, and the toolchain will tell you so if you write `false` and will
  tell you the opposite if you write a fact query — while proving, in the same run, that
  it knows the fact query is dead.

#### T4.6 — relation names are the one identifier class with no did-you-mean — ERGONOMIC

- **Intent** — the assignment's typo probes: the checks that decide whether a declared
  relational layer pays for itself.
- **Result — everything is caught, at the right severity, with the right body:**
  ```
  start="holds(can_hlat(toma))"
    8:56: error [E-RELATION-UNKNOWN] unknown relation `can_hlat` (dsl 0.3.0 §4)

  start="holds(can_halt(toma, extra))"
    8:56: error [E-RELATION-ARITY] relation `can_halt` expected 1 argument(s), got 2 (dsl 0.3.0 §4/§5)

  start="holds(can_halt())"
    8:56: error [E-RELATION-ARITY] relation `can_halt` expected 1 argument(s), got 0 (dsl 0.3.0 §4/§5)

  start="holds(can_halt(shed_sequence))"          # right arity, wrong entity kind
    8:56: error [E-FACT-DOMAIN] `shed_sequence` is not a declared member of entity kind
          `crew` (relation `can_halt` argument 0, dsl 0.3.0 §3.1)

  start="can_halt(toma)"                          # forgot the holds()
    8:56: error [E-CEL-PROFILE] `can_halt(…)` is outside the Lute-CEL profile — only
          operators, literals, lists, `?:`, `in`, `has()`, `isSet()`, `holds()`,
          `count()`, `validAt()`, and `now()` are permitted (dsl §8.4, 0.3.0 §8)
  ```
  All exit 1. Five failure modes, five codes, and the `E-CEL-PROFILE` one enumerates the
  entire permitted set, which is how I confirmed `count()` and `validAt()` are available
  here without opening a page. Unlike T3.4's `::assert` probes these are
  **column-exact** — `8:56` lands on the attribute value, not the start of the element.
- **The gap, and it is visible in a single run of a single file.** A misspelled state
  path in `done=` gets a suggestion; a misspelled relation in `start=` does not:
  ```
  9:66: error [E-UNDECLARED] state path `run.shedPresure` is not declared in `state:`
        (dsl §9.4) — did you mean `run.shedPressure`?
  8:56: error [E-RELATION-UNKNOWN] unknown relation `can_hlat` (dsl 0.3.0 §4)
  ```
  Same document, same check, same closed declared set to compare against — `relations:`
  has four members in `world.schema.yaml`. T3.4 recorded `E-RELATION-UNKNOWN` on `awak`
  and noted only its span; the missing suggestion is the more useful half, and it
  generalises: state paths, `after:` scene keys (`E-CONN-UNKNOWN-NODE`, T3.11) and
  `::set` targets all suggest; relation names alone do not.
- **Secondary, small: the warning fires over a query that does not typecheck.** On
  `start="holds(can_halt(toma, extra))"` the run emits both the `E-RELATION-ARITY` error
  *and* `W-UNPROVEN-RELATIONAL` at the same span, i.e. it reports that a malformed query
  is neither proved nor refuted. (`can_hlat` correctly emits no warning — the relation
  never resolves.) Cosmetic, but it is one more instance of the project-wide pass not
  knowing what the document pass already decided.
- **Verdict** — `ERGONOMIC`. Nothing is unexpressible and nothing is misreported; the
  cost is one extra round trip on the identifier class where a closed declared set makes
  the suggestion cheapest to compute. Recorded because the assignment asks directly
  whether these checks make the declared relational layer worth its cost, and the answer
  is an emphatic yes with one uneven edge.

#### T4.7 — nothing an author can run answers "is this quest reachable?" — ERGONOMIC

- **Intent** — the assignment's structural question. The quest lives in its own file with
  its own `uses:`, gating on a fact a scene two documents away asserts inside a
  `<choice>` arm. Nothing links them syntactically. So: how would an author know this
  quest is reachable at all?
- **Attempt** — every read-only surface the CLI offers, against the committed project.
- **Result — three tools, three partial answers, and the union still misses:**
  - **`lute scenario <dir>`** — the quest is not in the graph. Not a node, not a layer,
    not an edge; `--format json` has no `quest(holdTheSpine)` entry at all. This is
    *documented and deliberate* — `scene-graph.md`: "A quest becomes a graph node by
    declaring `after` (even `after=""`); a quest that never opts into a graph position is
    still addressable by `lute scenario <dir> envelope quest:<id>`, but contributes no
    edges." Confirmed by adding `after="visited('anseo.s01ep02')"`, which puts
    `quest(holdTheSpine)` in layer 2 with an edge from the scene. So the answer is
    available — but only to an author who already knew the answer and hand-declared it.
  - **`lute scenario <dir> reach quest:holdTheSpine`** — `verdict: Reachable`, with no
    mention of the `start` gate. Correct in the tool's own narrow sense (`--help`:
    "Evaluates no CEL, runs no Datalog"), and for a *scalar-dead* quest it does better
    than that (T4.5). For a fact-gated quest the word "Reachable" is the answer to a
    different question than the author asked.
  - **`lute scenario <dir> envelope quest:holdTheSpine`** — Guaranteed/Possible tables
    over `run.shedPressure` and `run.vesnaTrust`. **No fact section of any kind**, so the
    one gate that decides whether this quest ever activates is absent from the surface
    designed to say what holds when control reaches it. It does close with a genuinely
    useful nudge — "this quest declares no `after` attribute, so this is the defaults-only
    `D` table … declaring `after` … would enrich this table" — which is the clearest
    pointer in the toolchain toward the `after=` opt-in above.
  - **`lute trace`** — the best of the four at *locating* the question and, per T4.4,
    unable to answer it.
  - **`lute doctor`** — file counts and vocabulary slots; nothing relational.
- **The information exists.** `check-project` computed, in the same run, that `can_halt`
  is producible *because* `cryobank.lute` asserts `awake` and `knows` and the rule closes
  over them (T4.2's negative control proves the dependency is real and cross-document).
  That is precisely a producer → consumer edge, it is exactly what the author's question
  asks for, and no output renders it. Adding `after=` does not render it either — it
  records a second, hand-written claim that happens to run parallel to it.
- **Resolution** — I know the gate is live because T3.3 compiled the artifact and read
  the `assert` records out of it, then checked the rule by hand. That is the right way to
  confirm it and the wrong way to learn it.
- **Verdict** — `ERGONOMIC`. Not `TOOL-DEFECT`: every tool here is accurate within its
  documented scope, `scenario --help` and `scene-graph.md` both state their limits
  plainly, and the `envelope` note actively points at the fix. Not `DOC-GAP`: the pages
  say what the tools do. The cost is that quest/scene separation is real — separate file,
  separate `uses:`, no syntactic link — and the toolchain has the join in hand and
  renders it nowhere, so the author's obvious question has a four-command answer that
  still requires reading an artifact. In an eleven-scene work with six quests (Task 9),
  that is the surface that decides whether the quest layer is trustworthy.
- **Recommendation carried forward to Task 9**, recorded here rather than acted on
  because the brief specifies the committed file: giving each quest an `after=` costs one
  attribute and buys a graph node, a real edge, the full envelope table instead of the
  defaults-only one, and a `scenario` rendering an author can read. `hold-the-spine.lute`
  as committed has no `after=`, matching the brief.

#### T4.8 — the quest's relational gate reaches the artifact as an unparsed string — DOC-WRONG

- **Intent** — read the compiled quest back and confirm the gate survives to the runtime
  in a form a consumer can act on. The static layer is superb (T4.2, T4.3); the artifact
  is the other half of "does declaring `entities:` earn its keep".
- **Attempt** — `lute compile docs/examples/anseo/quests/hold-the-spine.lute --project docs/examples/anseo`.
- **Result** — the quest lowers to one `quest` command with its objectives inline, and
  the two predicate slots are **not** treated alike:
  ```json
  {"kind":"quest","addr":"001-0100","id":"holdTheSpine","title":"Hold the Spine",
   "titleLineId":"holdTheSpine.title",
   "start":{"raw":"holds(can_halt(toma))"},
   "objectives":[{"id":"reachToma","title":"Reach the spine coupling",
     "titleLineId":"holdTheSpine.reachToma",
     "done":{"raw":"run.shedPressure >= 1",
             "expr":{"op":">=","l":{"path":"run.shedPressure"},"r":{"lit":1.0}}},
     "optional":false,"body":null}]}
  ```
  The scalar `done` carries a parsed `expr` tree beside its `raw`; the relational `start`
  carries `raw` only. A consumer written against the `expr` AST gets `undefined` on every
  fact gate and must parse `holds(can_halt(toma))` itself. (Consistent with T3.5, where a
  `<choice when="!holds(awake(toma))">` also reached the artifact `expr`-less while the
  scalar guard beside it did not.)
- **What the docs say, and they disagree with each other.**
  - `tooling/runtime-contract.md:22`, the Lute-vs-engine responsibility table:
    **"Lower every CEL guard to a portable `expr` AST."** `holds(can_halt(toma))` is a
    CEL guard — it sits in a CEL slot and `E-CEL-PROFILE` lists `holds()` among the
    permitted forms — and it is not lowered.
  - `schemas/lute-ir-0.9.schema.json`, `$defs.exprNode`: `expr` is "**Absent** whenever
    the CEL slot was empty or fell outside the closed Lute-CEL profile (dsl §8.4)". So
    the machine-checkable schema correctly permits the absence — but its stated *reason*
    does not apply either, because `holds()` is squarely **inside** the §8.4 profile, by
    the profile error's own enumeration.
  So the artifact's behaviour is licensed by the schema and contradicted by the prose,
  and neither states the actual rule: *relational fact queries lower to `raw` only.*
- **Resolution** — none needed authorially; the committed quest is unaffected and the
  reference runtime handles it (`lute trace` resolves the gate from seeds). The finding
  is for whoever writes the second engine.
- **Verdict** — `DOC-WRONG`, ranked per the table above `DOC-GAP`. The runtime contract
  is the page an engine implementer reads to know what they must handle, its statement is
  present and universally quantified ("every CEL guard"), and it is false for the exact
  construct this task exists to demonstrate. An implementer who believes it writes
  `evalExpr(cmd.start.expr)` — the page's own §"the engine loop" pseudocode does exactly
  this for `set`/`choice`/`match` — and never discovers they were lied to until a fact
  gate silently evaluates undefined. One clause on line 22 ("every CEL guard *except*
  relational fact queries, which carry `raw` only") closes it.

#### T4.9 — small things, recorded once

- **Scene-only frontmatter keys are rejected per key, exactly as the brief predicted —
  worked well.** `character`/`season`/`episode` in a quest document:
  ```
  1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `character` (not a core key
       and not owned by an active plugin)
  ```
  Three errors for three keys, not one roll-up, so pasting a scene header into a quest is
  a single-pass fix. The message's "not owned by an active plugin" clause is the useful
  half — it says *why* the key is unknown rather than just that it is.
- **Quest identity is derived from the quest id, and titles are addressable — worked
  well.** With no `character`/`season`/`episode` to build `{prefix}` from, the `<on>`
  arm's narration lands on `"lineId": "holdTheSpine.narrator_0010"`, and both the quest
  title and each objective title get a `titleLineId` (`holdTheSpine.title`,
  `holdTheSpine.reachToma`) — so quest-log strings are localisable on the same footing as
  dialogue. Neither is mentioned by `lute context` or on the identity docs (cf. T1.7).
- **`scenario envelope` leaks internal vocabulary into author-facing output.** The table
  is annotated: *"This is NOT the T11 warning-grade read-site class -- quest read
  diagnostics are `check_quest_guard_defassign`'s separate territory (that class is
  scene-only, see the scene envelope's own section)"*. `check_quest_guard_defassign` is a
  Rust function name and "T11" is an internal task label; neither appears anywhere on the
  website. Trivial in cost and the surrounding output is good, but it is the same habit
  as T1.4's fabricated `::narrator` — CLI output describing the author's project in terms
  only the compiler's authors can resolve.

#### T4 summary

Nine entries: four *worked well*, two `TOOL-DEFECT`, two `ERGONOMIC`, one `DOC-WRONG`.
No `LANGUAGE-GAP`, no `DOC-GAP`, no `AUTHOR-ERROR`. Nothing this quest wanted was
inexpressible and nothing was substituted — the sequenced objective, the optional
objective, the independent failure condition and the derived-relation gate were each
written in the form first reached for, and each worked (T4.1).

**The declared relational layer pays for itself, and the receipt is T4.2.** A quest in
its own file, with its own `uses:`, gated on a Datalog head whose base facts are asserted
inside a `<choice>` arm of a different episode — and `check-project` still decides
whether that gate can ever open, closes the rule set to do it, names the offending
relation, and flips warning→error when you delete the producer from the other document.
Set beside T4.3's `E-FACT-DOMAIN` on `nobody` and T4.6's five distinct codes for five
distinct malformations, this is the strongest analysis surface measured in four tasks.
No string-keyed flag design computes any of it.

**And it is reported to the author through four surfaces, three of which are wrong.**
The pattern is identical to T1–T3's and it is now unmistakable: *this toolchain computes
more than it will tell you, and where it tells you, it sometimes tells you the opposite.*
T4.5 is the one to fix first and it is the worst thing in this log after T3.2 — the same
producibility judgement that makes `done="holds(found(toma))"` a build-breaking error
makes `start="holds(found(toma))"` emit nothing at all, and makes `scenario reach` print
**Reachable** for a quest that can never activate, while `start="false"` correctly prints
`Unreachable` citing `E-QUEST-UNREACHABLE`. The analysis, the slot, the diagnostic class,
and the spec clause (`dsl 0.4 §5.3`) are all already there; one branch is missing, and
its absence makes a dead quest quieter than a live one.

T4.4 is second and it is the more demoralising, because it is what an author hits *doing
everything right*. `W-UNPROVEN-RELATIONAL` is a well-written warning that states a real
boundary and names a remedy — and the remedy cannot be performed: `lute trace` will not
run the rule the gate depends on (documented), and when you seed the conclusion instead
it declares the seed unproducible using a document-local judgement that contradicts the
project-wide one in the warning that sent you there. On the assignment's question, then:
the warning is not a shrug and it does not train people to ignore warnings by being
noisy — it trains them by being undischargeable. Six of them now sit on
`check-project docs/examples`, and every one marks a correct, deliberate gate.

The remaining three are cheaper: no did-you-mean on relation names, alone among the
identifier classes that have a closed declared set (T4.6); a quest that is invisible to
`lute scenario` until an author hand-writes an `after=` the checker has already inferred
the substance of (T4.7); and a runtime-contract table promising an `expr` AST for "every
CEL guard" while relational gates ship as `raw` strings (T4.8).

One thing later tasks must carry forward: **`lute check <file>` is not enough for a
quest.** `E-OBJECTIVE-UNSATISFIABLE`, `W-UNPROVEN-RELATIONAL` and `W-QUEST-REF-UNKNOWN`
are all project-wide, so a quest with a typo'd cross-quest reference —
`quest.holdTheSpine.objectives.reachTomaTYPO.done` — is `ok: … (0 warning(s))`, exit 0,
under per-file `check --deny-warnings`, and only `check-project` reports it (it does, and
well: distinct message bodies for an unknown quest id and for a known quest that does not
declare that objective). This generalises T3.11's caveat from `after:` to the whole quest
layer.
