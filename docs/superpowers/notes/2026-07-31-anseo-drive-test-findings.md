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
- **Verdict** — exactly one of the seven below. Never invent a verdict or hyphenate a
  hybrid (`AUTHOR-ERROR-adjacent` is not a verdict); if none fits, say so in the entry
  and raise it with the controller, who owns this table.

### Verdicts

| Verdict | Criterion |
|---|---|
| `LANGUAGE-GAP` | The intent cannot be expressed. **Two sufficient shapes, either one alone qualifies:** (a) you changed the story to fit the tool, or (b) only a lossy proxy exists — the intent is reachable by encoding it as something else, but nothing in the language *means* it, so nothing can check it. Do not withhold this verdict merely because a workaround was found; say which shape applies and what the proxy costs. |
| `ERGONOMIC` | Expressible, but the working form is materially worse than the natural one — more verbose, more indirect, or split across files for no modelling reason. |
| `DOC-GAP` | Expressible and reasonable, but **you had to read Rust source, a proposal, or a test to find it.** The website docs and `lute context` did not get you there. |
| `DOC-WRONG` | The docs are present and **false** — they state a restriction that does not exist, a behaviour that differs, or scope something to the wrong construct. Distinct from `DOC-GAP`, which is silence: silence makes an author search, a false statement makes them stop searching. Rank these above `DOC-GAP` by default; an author who believes a wrong doc never discovers they were lied to. |
| `AUTHOR-ERROR` | The docs said so plainly and you missed it. Not a finding — record it only if the diagnostic pointed somewhere unhelpful. |
| `TOOL-DEFECT` | The language and its docs are fine; a *tool* is wrong, incomplete, or lying about its own contract. A misdirecting diagnostic, a false green, a capability surface that omits something it advertises. Distinct from `DOC-GAP`: the information exists, but the tool that promised to hand it to you did not. |
| `SPEC-WRONG` | Everything works as designed and the design is the defect. Language, docs, and every tool agree; the specified behaviour is itself the wrong call. Use this when you cannot fault any implementation and still believe an author is badly served — a severity chosen wrongly, two equivalent proofs given unequal treatment, a default that surprises. State what the spec says, why it is wrong, and what it should say instead; this verdict is worthless without a proposed alternative. |

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

#### T4.4 — `W-UNPROVEN-RELATIONAL` names two verification routes, and the tool one cannot do the job — TOOL-DEFECT

The assignment asks whether this warning is actionable or a shrug the author learns to
ignore. It is neither: it is a referral, and it names two routes — `lute trace` seeds
**or human review**. The tool route is the one this entry measures, and it does not work.

- **The warning, in full:**
  ```
  warning [W-UNPROVEN-RELATIONAL] `start="holds(can_halt(toma))"` is gated by a
  relational fact query over producible relation(s) `can_halt`; static reachability
  analysis (dsl 0.6.1 §2) neither proves nor refutes it. Verify with `lute trace`
  seeds or human review
  ```
  As prose this is close to a model diagnostic: it quotes the offending attribute, names
  the relation, cites the clause, states the limit precisely ("neither proves nor
  refutes"), and — unusually for a `W-` code — **names remedies**, two of them. It is not
  a shrug. It is a referral.
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
  defect. But the consequence is that the only **tool-assisted** route the warning offers
  requires you to **assert the conclusion**, and the rule
  `can_halt(C) :- awake(C), knows(C, shed_sequence)` — the thing the whole quest rests
  on, and the only part of the chain a human could plausibly get wrong — is never
  evaluated by any command an author can run.
- **(b) And when you do supply the conclusion, trace tells you it proves nothing.**
  ```console
  $ lute trace quests/hold-the-spine.lute --project docs/examples/anseo --fact "can_halt(toma)"
  trace: … (seeds: 0 paths, 1 facts; 0 selections)
  note: W-TRACE-MOCK-UNPRODUCIBLE — mock fact over relation `can_halt` is not producible
  (no `facts:` seed, no reachable `::assert`, not `reserved`) — the supplied answer can
  never arise from authored producers, so a complete walk seeded with it proves nothing
  about reachable play (§4)
    <quest holdTheSpine>   -> active (holds(can_halt(toma)))
    <objective reachToma>   -> pending (run.shedPressure >= 1)
  trace complete: 2 decisions                                              # exit 0
  ```
  The referral's tool-assisted half closes the loop back onto itself: `check-project` says
  "verify with `lute trace` seeds", and `lute trace` says the seed proves nothing.
  *(Correction of record. The first pass logged `trace complete: 4 decisions` against this
  command. The committed one-objective quest emits **2** — one quest decision, one
  objective — and the transcript above is the re-run, verbatim. The 4 is T4.1's richer
  three-objective scratch form, re-confirmed on a rebuilt scratch copy: quest +
  `reachToma` + `cutCoupling` + `pullManifest`. Everything else in this entry reproduced.)*
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
- **The other named route is human review, and nothing here shows it is impossible.** The
  warning says "Verify with `lute trace` seeds **or human review**", and only the first of
  the two is a tool. Human review is in fact what discharged this gate: T3.3 compiled the
  artifact, read the `assert` records out of it, and checked the rule by hand. So the
  claim this entry proves is the narrow one — **the tool-assisted route is unusable** —
  and the offered fallback is unassisted manual work over a compiled artifact, on the one
  link in the chain (`can_halt(C) :- awake(C), knows(C, shed_sequence)`) that the
  toolchain has already reasoned about, correctly and at relation level, and renders
  nowhere (T4.7).
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
  contract". The false claim is (c): `trace` reports a **document-local** `producible()`
  verdict in project-wide language, contradicting `check-project` on the same word, in the
  same project, under the same `--project` root. (b) is not a second, independent lie —
  it is that same one closing the loop, the referral's tool half answered by a false claim
  about your project. **On the assignment's question:** the warning is *not* a noise floor
  that trains people to ignore warnings — it fires on exactly the correct usages, but it
  fires with a specific, honest, quotable statement of an analysis boundary, which is the
  right thing for a checker to do when it cannot decide. Five such warnings already sit on
  other examples and `check-project docs/examples` still exits 0 with all six. What erodes
  it is narrower than "undischargeable": of the two routes the warning names, the one an
  author reaches for first — the tool it names by command — cannot be completed, so every
  firing falls back on unassisted review of a compiled artifact.

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
  as committed has no `after=`, matching the brief. **Adopted** — see *T4 controller
  decision* below for the ruling and for why T4 stays the no-`after=` control.

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

#### T4.9 — a quest document's frontmatter and its identity chain both behave — WORKED WELL

Two observations, one disposition, one verdict.

- **Intent** — get a `kind: quest` document's header and its localisable strings right in
  a project whose only prior documents are scenes, and find out what a quest's identity is
  built from when the scene keys that build `{prefix}` are unavailable.
- **Attempt** — (i) the scene header pasted verbatim into the quest, `character`/`season`/
  `episode` and all; (ii) the committed quest compiled, and its identity fields read back
  out of the artifact.
- **Result** — both behaved:
  - **Scene-only frontmatter keys are rejected per key, exactly as the brief predicted:**
    ```
    1:1: error [E-META-UNKNOWN-KEY] unknown top-level meta key `character` (not a core key
         and not owned by an active plugin)
    ```
    Three errors for three keys, not one roll-up (exit 1), so pasting a scene header into
    a quest is a single-pass fix. The message's "not owned by an active plugin" clause is
    the useful half — it says *why* the key is unknown rather than just that it is.
  - **Quest identity is derived from the quest id, and titles are addressable.** With no
    `character`/`season`/`episode` to build `{prefix}` from, the `<on>` arm's narration
    lands on `"lineId": "holdTheSpine.narrator_0010"`, and both the quest title and each
    objective title carry a `titleLineId` (`holdTheSpine.title`, `holdTheSpine.reachToma`)
    — so quest-log strings are localisable on the same footing as dialogue.
- **Resolution** — the committed frontmatter carries `kind`/`luteVersion`/`uses`/`title`
  only. Nothing was worked around.
- **Verdict** — worked well. One caveat, deliberately **not** scored here: neither the
  quest `{prefix}` derivation nor `titleLineId` is mentioned by `lute context` or on the
  identity docs. That is T1.7's existing `DOC-GAP` extended to quests — cross-referenced,
  not counted a second time.

#### T4.10 — `scenario envelope` describes the author's project in compiler-internal vocabulary — TOOL-DEFECT

- **Intent** — read the envelope as an author would, to learn what state is safe to read
  when the quest activates. Part of the T4.7 sweep, split out because it is a defect in
  the output rather than a limit of the surface.
- **Attempt** — `lute scenario docs/examples/anseo envelope quest:holdTheSpine`.
- **Result** — the `Possible \ Guaranteed` table is annotated:
  ```
  Possible \ Guaranteed -- inventory only (paths set on SOME but not every declared route
  reaching this quest, dsl §4.4). This is NOT the T11 warning-grade read-site class --
  quest read diagnostics are `check_quest_guard_defassign`'s separate territory (that
  class is scene-only, see the scene envelope's own section)
  ```
  `check_quest_guard_defassign` is a Rust function name and "T11" is an internal task
  label; neither appears anywhere on the website. The sentence is addressed to a reader
  with the compiler's source and its task tracker open, and it is printed to an author.
- **Resolution** — `NONE — nothing to resolve; the table itself is correct and the
  committed project is unaffected.`
- **Verdict** — `TOOL-DEFECT`, and the smallest of T4's three by a wide margin. Not
  `LANGUAGE-GAP` or `ERGONOMIC` — nothing about the authored form is at issue. Not
  `DOC-GAP` or `DOC-WRONG`: no page is silent or false, and no page *could* fix this, since
  the two terms are absent from the docs precisely because they are not public API. Not
  `AUTHOR-ERROR`. That leaves the criterion's own words — a tool wrong about its own
  contract, where the contract of author-facing output is that an author can resolve it.
  Same habit as T1.4's fabricated `::narrator`; cosmetic in cost, recorded because the
  habit is now four tasks old.

#### T4 controller decision — Task 9's quests carry `after=`; this one deliberately does not

Recorded in the durable log rather than left in scratch, because a future reader comparing
`hold-the-spine.lute` with Task 9's five quests will otherwise read the difference as an
oversight. The implementer asked whether quests should carry `after=`. The decision, taken
on the reviewer's recommendation and independently verified by them:

> **The five Task 9 quests carry explicit `after=` prerequisites. `hold-the-spine.lute` is
> not retrofitted.**

The reasoning is T4.7's measurement, and both halves reproduce on the committed project:

- Without `after=`, a quest is absent from the `lute scenario` graph *entirely* — no node,
  no layer, no edge — and `envelope quest:holdTheSpine` returns the **defaults-only `D`
  table**, closing with its own note that declaring `after` "would enrich this table".
- With `after="visited('anseo.s01ep02')"`, `quest(holdTheSpine)` appears at **layer 2**
  with a real `scene(anseo.s01ep02) -> quest(holdTheSpine) [visited]` edge, and the
  defaults-only note disappears — the envelope is now the project-resolved one.

So `after=` costs one attribute and buys the reachability surface that Task 9's
eleven-scene, six-quest shape needs, on all five of its new quests.

Keeping T4 as the **no-`after=` control is deliberate**, not an inconsistency. It leaves
the blind spot visible in a shipped example: a quest that is genuinely reachable,
genuinely checked project-wide (T4.2), and invisible to the one tool an author would ask
about reachability (T4.7) — while `scenario reach` on the committed tree still answers
*"Reachable — a plain quest with no declared `after` prerequisite"*. That visibility is
worth more than uniformity across the two tasks, and it keeps T4.5's `reach` probe one
attribute away from the committed tree rather than requiring the `after=` line be stripped
again first.

#### T4 summary

Ten entries: four *worked well* (T4.1, T4.2, T4.3, T4.9), three `TOOL-DEFECT` (T4.4, T4.5,
T4.10), two `ERGONOMIC` (T4.6, T4.7), one `DOC-WRONG` (T4.8) — every entry carrying
exactly one verdict. No `LANGUAGE-GAP`, no `DOC-GAP`, no `AUTHOR-ERROR` scored here; the
one `DOC-GAP`-shaped observation T4 turned up (quest identity is undocumented) extends
T1.7 and is counted there. Nothing this quest wanted was inexpressible and nothing was
substituted — the sequenced objective, the optional objective, the independent failure
condition and the derived-relation gate were each written in the form first reached for,
and each worked (T4.1).

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
boundary and names two remedies — `lute trace` seeds **or human review** — and the tool
one cannot be performed: `lute trace` will not run the rule the gate depends on
(documented), and when you seed the conclusion instead it declares that seed unproducible
on a document-local judgement that contradicts the project-wide one in the warning that
sent you there. The human-review fallback does stand, and it is what actually discharged
this gate (T3.3) — but discharging it means reading a compiled artifact by hand, every
time the warning fires, for a producer→consumer join the checker already computed and
renders nowhere (T4.7). On the assignment's question, then: the warning is not a shrug and
it does not train people to ignore warnings by being noisy. The route an author reaches
for first is simply closed. Six of them now sit on `check-project docs/examples`, and
every one marks a correct, deliberate gate.

The remaining four are cheaper: no did-you-mean on relation names, alone among the
identifier classes that have a closed declared set (T4.6); a quest that is invisible to
`lute scenario` until an author hand-writes an `after=` the checker has already inferred
the substance of (T4.7, and see the controller decision above); a runtime-contract table
promising an `expr` AST for "every CEL guard" while relational gates ship as `raw` strings
(T4.8); and `scenario envelope` annotating an author-facing table with a Rust function
name and an internal task label (T4.10).

One thing later tasks must carry forward: **`lute check <file>` is not enough for a
quest.** `E-OBJECTIVE-UNSATISFIABLE`, `W-UNPROVEN-RELATIONAL` and `W-QUEST-REF-UNKNOWN`
are all project-wide, so a quest with a typo'd cross-quest reference —
`quest.holdTheSpine.objectives.reachTomaTYPO.done` — is `ok: … (0 warning(s))`, exit 0,
under per-file `check --deny-warnings`, and only `check-project` reports it (it does, and
well: distinct message bodies for an unknown quest id and for a known quest that does not
declare that objective). This generalises T3.11's caveat from `after:` to the whole quest
layer.

### T5 — The terminators

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. Two scenes added:
`scenes/bridge.lute` (`anseo.s01ep10`, the success terminal) and `scenes/shed.lute`
(`anseo.s01ep11`, the failure terminal) — the corpus's **first two `::end`s**. Nothing
under `docs/examples/` had ever terminated a walk before, so every entry below is a
first-use measurement. Both `after:` routes are provisional (Task 8 repoints them).

#### T5.1 — `::end` works, first try, on every surface that carries it — WORKED WELL

- **Intent** — two endings. Vesna reaches the bridge and the ship still steers; or the
  Purser gets its module and the allocation is satisfied. Each beat stops there.
- **Attempt** — the brief's two files verbatim, `::end{reason="bridge-reached"}` and
  `::end{reason="shed-with-module"}` as the last node of each document.
- **Result** — `ok: docs/examples/anseo (5 file(s), …)`, exit 0. Every surface that is
  supposed to carry the terminator does:
  - **the artifact** — `{"kind":"end","addr":"001-0400","reason":"bridge-reached"}`, an
    ordinary addressed record at the normal +100 gap, no `wait`/`duration` stamp;
  - **`lute run`** — `001-0400  end    reason=bridge-reached`, then `run complete`;
  - **`W-CODE-AFTER-END`** — fires exactly as `directives.md` describes, once, anchored
    at the first dead node rather than at the `::end`, with the spec's message verbatim;
  - **`--deny W-CODE-AFTER-END`** — promotes it to `error [W-CODE-AFTER-END] [denied]`,
    exit 1.
  - **`E-UNKNOWN-ATTR`** — `::end{ending="…"}` is `` `::end` has no attribute `ending` ``.
    The attribute *key* is closed even though its value is not (T5.3).
- **Resolution** — n/a; nothing was substituted.
- **Verdict** — worked well. This is the least-exercised construct in the language and
  it behaved like the best-exercised one. Worth saying plainly before the four entries
  that follow, all of which are about what `::end` *does not* mean rather than about
  anything it got wrong.

#### T5.2 — the same JSON document carries two unrelated `reason` fields, and the obvious verification matches the wrong one — ERGONOMIC

- **Intent** — confirm the authored reason survives to the artifact.
- **Attempt** — the first thing anyone types:
  ```console
  $ lute compile docs/examples/anseo/scenes/bridge.lute -o /tmp/t5.json
  $ grep -n '"reason"' /tmp/t5.json
  217:        "reason": "pre-loading `vesna`'s first emotion `level` seen ahead of the entrance"
  233:      "reason": "bridge-reached"
  ```
- **Result** — two `reason` keys, and the **first** one is not mine. `bridge.lute`'s
  `::auto` triggers `entry-emotion-lookahead`, whose injected preload sprite carries
  `provenance: { injected, by, reason }` — and that `reason` is a *human-readable English
  justification for a compiler decision*. Mine is an *opaque author token for a host to
  dispatch on*. Same key, same document, one nested under `provenance` and one not,
  contracts with nothing in common. `grep -m1`, `jq '..|.reason?|select(.)' | head -1`,
  and any harness that greps the file all read the injector's prose and report success.
- **Resolution** — match the record, not the key:
  `jq -c '.commands[] | select(.kind=="end")'` → `{"kind":"end","addr":"001-0400","reason":"bridge-reached"}`.
- **Verdict** — `ERGONOMIC`. Not `TOOL-DEFECT`: both shapes are documented, both are
  where they should be, and a *correct* consumer walking `commands[]` by `kind` never
  sees the collision. The cost is entirely on ad-hoc verification, which is what an
  author and an AI harness actually do — and the evidence that the cost is real is that
  this task's own brief had to carry a warning sentence about it. `provenance.reason` is
  the field with the weaker claim on the name (it is a `why`, not a `what`); calling it
  `note` or `justification` would end the collision for free.

#### T5.3 — `::end`'s `reason` is unconstrained, and the one thing an author reaches for to constrain it is accepted, advertised as live vocabulary, and inert — TOOL-DEFECT

This is T5's most serious finding.

- **Intent** — a work with a closed set of endings wants its ending ids closed too. Every
  other value in Anseo's content is checked against a declared domain (T1.2, T1.3), so the
  natural instinct is to declare one for the ending ids and get the same protection.
- **Attempt, in order.** All probes on a scratch copy of the project.
  1. **Is `reason` required?** `::end` bare → `ok`, 0 warnings. Documented (`reason` is
     optional), so not a finding — but it means a terminator can carry no identity at all
     and the artifact simply omits the field.
  2. **Duplicate reason across both documents.** Set `bridge-reached` on the shed
     terminator too, so two different endings of one project answer to one id →
     `check-project` clean, 0 project-wide diagnostics. No cross-document notion of an
     ending id exists to collide.
  3. **Misspelled reason.** `::end{reason="bridge-reachd"}` → `ok`, exit 0. Nothing to
     spell it against.
  4. **Empty reason.** `::end{reason=""}` → `ok`, and it reaches the artifact as
     `{"kind":"end","addr":"001-0200","reason":""}`. A host that distinguishes "no reason
     given" (field absent) from "reason given" now has a silent third state: field
     present, empty.
  5. **Declare a domain for it** — the actual attempt, and the one that matters:
     ```yaml
     # world.schema.yaml
     enums:
       reason: [bridge-reached, shed-with-module]
     ```
- **Result** — the declaration is accepted with no diagnostic (no `E-DOMAIN-DUP`, no
  `E-DOMAIN-UNKNOWN`), it **constrains nothing** — `::end{reason="not-a-declared-member"}`
  still checks `ok`, exit 0 — and it is then *advertised as live vocabulary by two
  surfaces*:
  ```console
  $ lute context scenes/p1.lute
  projectEnums (8):
    action: brace, drift, turn-away, seal, unseal, step-out, go-under
    anchor: port, center, starboard
    emotion: level, clipped, frayed, hollowed, wry, stricken
    mood: quiet, pressurized, failing, weightless
    musicAction: start, swell, cut, resume, fade-out
    reason: bridge-reached, shed-with-module      <-- enforces nothing
    vfxType: shed, klaxon, pressure-drop, frost
    volume: silent, muted, normal, raised, alarm
  ```
  ```console
  $ jq -c '.enums[] | select(.name=="reason")' artifact.json
  {"name":"reason","members":["bridge-reached","shed-with-module"]}
  ```
  `reason` sits in `projectEnums` beside the seven slots that *are* enforced, with no mark
  distinguishing it, and ships into the compiled artifact's `enums` array — the array
  `vocabulary.md` describes as making "the artifact self-describing about the vocabulary it
  was compiled against". The artifact asserts it was compiled against a two-member `reason`
  domain. It was not compiled against it at all.
- **Generality** — not specific to the name. `enums: sausage: [a, b]` behaves identically:
  accepted, listed in `projectEnums`, shipped in `enums`, read by nothing. Any project may
  declare arbitrary dead vocabulary and both surfaces will vouch for it.
- **Resolution** — `NONE — intent abandoned.` The shipped corpus leaves `reason` as a free
  string. The mirrored-state proxy in T5.4 is the closest thing to typing an ending, and it
  does not type `reason`.
- **Verdict** — `TOOL-DEFECT`. The language is honest: `directives.md` says `reason` is
  "optional and free-form … Lute assigns it no meaning", and 0.8.0 §3 agrees. So the
  *absence* of typed end reasons is a documented design choice and not a finding. What is a
  finding is that the two surfaces an author and a harness use to learn what is enforced —
  `lute context`, whose `--help` calls itself the surface "an AI needs to WRITE valid Lute",
  and the artifact's self-describing `enums` array — both report an enforced domain that is
  not enforced. This is T1.6's defect with the sign flipped: there `context` omitted things
  the project really had; here it invents one the project really does not. An author who
  declares the domain, sees it in `context`, sees it in the artifact, and concludes their
  ending ids are now typed has been told so by two tools and is wrong. A `W-DOMAIN-UNREAD`
  on a declared domain no active construct reads would close it, and the checker already
  knows the reading set — it computes `E-DOMAIN-UNKNOWN` from exactly that.

#### T5.4 — nothing says two endings are one story's alternation, and nothing says one of them is the bad one; both are reachable only by mirroring the ending into declared state and saying it twice — LANGUAGE-GAP

The assignment's two hardest questions, and they turn out to be one question. Both probes
were run to a working end before this verdict was assigned.

- **Intent** — two things a work with endings wants to state. (a) *These are the endings of
  this story* — a set, a category, an exhaustiveness claim, something that makes adding a
  third ending a change to a declared list rather than a new string in a new file. (b)
  *This one is the failure* — `::end{reason="shed-with-module"}` says the shed happened; it
  does not say it went badly.
- **Attempt (a), the ending set.** Four routes, in the order I reached for them:
  1. **Frontmatter.** `ending: true`, `terminal: true`, `outcome: failure`, and
     `endings: [bridge-reached, shed-with-module]` on the scene →
     `error [E-META-UNKNOWN-KEY] unknown top-level meta key `ending` (not a core key and
     not owned by an active plugin)` for each. A closed key set is the right design and the
     diagnostic even names the escape hatch — but the escape hatch is *ship a plugin*.
  2. **A `reason` enum domain.** T5.3: accepted, advertised, inert.
  3. **The graph.** T5.5: no notion of termination anywhere in it.
  4. **Mirror it into declared state** — the one that works:
     ```yaml
     run.ending: { type: { enum: [unspecified, bridge-reached, shed-with-module] }, default: unspecified }
     ```
     ```lute
     ::set{run.ending = "bridge-reached"}
     ::end{reason="bridge-reached"}
     ```
     and then the claim becomes checkable, because `<match>` exhaustiveness is real: a
     `<match on="run.ending">` covering one arm is
     `error [E-NONEXHAUSTIVE] non-exhaustive `<match>`: the subject's domain is not fully
     covered and there is no `<otherwise>`` plus `E-UNSET-UNCOVERED`. Add a third ending to
     the enum and every reader of it breaks until it is handled. That is exactly the
     property (a) wanted.
- **Attempt (b), the failure ending.** `::end` declares one attribute, so there is nothing
  to write on it (`E-UNKNOWN-ATTR`, T5.1). The language's actual failure vocabulary is the
  quest lifecycle, so: a quest whose `fail=` reads the mirrored enum.
  ```lute
  <quest id="theWalk" title="The Walk" start="run.shedPressure >= 0" fail="run.ending == 'shed-with-module'">
  <objective id="reachBridge" title="Reach the bridge" done="run.ending == 'bridge-reached'"/>
  <on event="questComplete">
  @narrator: The ship still had a helm.
  </on>
  <on event="questFailed">
  @narrator: The allocation was satisfied. That was all.
  </on>
  </quest>
  ```
  Driven end to end (`compile --all`, then `run` the quest artifact with
  `state: {run.ending: shed-with-module}`):
  ```
  quest theWalk -> active
  quest theWalk -> failed
    001-0500  narrator: The allocation was satisfied. That was all.
  -- quests --
    theWalk: failed
  ```
  So (b) **is** expressible. Lute has genuine, typed, engine-observable failure semantics
  with a lifecycle event and a reserved readable path. They attach to a quest, not to an
  ending.
- **Resolution** — the shipped corpus keeps the two bare `::end{reason}`s. The proxy above
  is not in `docs/examples/anseo/` because paying its price for two endings in an
  eleven-scene prologue is not a call this task should make for Task 8, and recording what
  it costs is the deliverable. What it costs:
  1. **Every ending is stated twice, in two syntaxes, and nothing checks that they agree.**
     `::set{run.ending = "bridge-reached"}` and `::end{reason="bridge-reached"}` are
     unrelated strings on adjacent lines. Swap one and both check clean.
  2. **The half that is supposed to be typed is not.** T3.2's hole applies verbatim to enum
     paths — `::set{run.ending = "shed-with-modle"}` against a declared
     `{ enum: [bridge-reached, shed-with-module] }` is `ok`, exit 0. Verified here, not
     assumed. So the *entire* protection the proxy buys is `<match>` exhaustiveness at the
     read sites; the write sites are as unchecked as the `reason` strings they mirror.
  3. **A sentinel enum member exists only to satisfy the checker.** `run.ending` has no
     honest default, and without one every quest predicate reading it is `E-MAYBE-UNSET`
     (T5.6). A two-ending story therefore declares a three-member domain, and every
     exhaustive `<match>` over it carries an arm for a value that is not an ending.
  4. **Polarity lands on the quest, not the ending.** The `end` record in the shed artifact
     is still `{"kind":"end","reason":"shed-with-module"}`. A host reading the terminator —
     the record whose entire purpose is to tell the host how the walk ended — learns nothing
     about whether that was good. It must separately be running the quest layer and reading
     `quest.theWalk.state`.
  5. **Nothing observes the join.** The quest lives in its own document; `lute run` takes
     one artifact, so no shipped tool plays the scene and the quest together. This is
     T4.7's shape exactly and is counted there, not re-filed.
- **Verdict** — `LANGUAGE-GAP`, **shape (b)**, for both halves. Nothing in the language
  *means* either claim, so nothing can check either claim; each is reachable only by
  encoding it as something else.
  - **The proxy, named.** Ending identity becomes a declared enum state path —
    `run.ending: { type: { enum: [unspecified, bridge-reached, shed-with-module] } }` — written
    as a `::set` on the line above the `::end`. Ending polarity becomes a quest lifecycle
    transition, `fail="run.ending == 'shed-with-module'"` reading that same mirrored path.
    Both were driven to a working end before this verdict was assigned, and both work:
    `<match>` exhaustiveness genuinely breaks when a third ending is added
    (`E-NONEXHAUSTIVE` + `E-UNSET-UNCOVERED`), and `quest theWalk -> failed` is a genuine,
    typed, engine-observable failure. The evidence stands as recorded; none of it is
    softened by this reclassification.
  - **What the proxy costs.** Itemised above, and the first item is the one that makes this
    shape (b) rather than a verbose spelling: **no check connects either proxy to the
    adjacent `::end`.** `::set{run.ending = "bridge-reached"}` and
    `::end{reason="bridge-reached"}` are unrelated strings on consecutive lines; an
    intentional mismatch between them checks clean, verified here and independently
    reproduced in review. Then: the mirrored write site is itself untyped, so a misspelt
    enum member is `ok` at exit 0 (cost 2, T3.2 re-verified for enum paths); a two-ending
    story must declare a three-member domain because a sentinel exists only to satisfy the
    checker (cost 3, T5.6); the `end` record — the one record whose entire purpose is to
    tell a host how the walk ended — still says nothing about whether it ended well (cost
    4); and no shipped tool plays the scene and the quest together, so nothing observes the
    join (cost 5, T4.7). The corpus ships neither proxy: `docs/examples/anseo/` keeps two
    bare `::end{reason}`s, and both claims therefore go unstated in the delivered work.
  - **Why not `ERGONOMIC`.** `ERGONOMIC` is for a working form materially worse than the
    natural one, which presumes the language can say the thing at all. It cannot. `::end`
    declares one attribute (`E-UNKNOWN-ATTR`, T5.1); no frontmatter key admits the claim
    (`E-META-UNKNOWN-KEY` on `ending`, `terminal`, `outcome`, `endings`); a declared
    `reason` domain is accepted, advertised and inert (T5.3); and the scenario graph has no
    notion of termination to hang it on (T5.5). What the proxy produces is not the claim
    said awkwardly — it is a *different* claim, about a state path and a quest, which a
    reader has to trust corresponds to the terminator beside it.
  - **The amendment is what moved it.** This entry was first filed `ERGONOMIC`, with a note
    to the controller rather than a forced verdict, because **the story itself is fully
    expressible** — both endings are written, both play, both stop, nothing was substituted
    and no beat was dropped — and the then-current criterion's second sentence ("You
    changed the story to fit the tool") read as a precondition. The controller amended
    `LANGUAGE-GAP` so that either shape alone qualifies. Shape (b) is precisely this case:
    the work is intact, the claim *about* the work is not expressible, and only a lossy
    proxy reaches it. One optional attribute on `::end` — a declared-domain `reason`, or an
    `outcome` — would make both claims mean something, and would let something check them.

#### T5.5 — `::end` is not an ending, and no tool will tell you whether a route reaches one — DOC-WRONG

- **Intent** — the structural question a branching work lives or dies on. Two terminals now
  exist; ask the tooling (i) which nodes are terminals, (ii) whether every route reaches
  one, (iii) whether a route can dead-end without terminating.
- **Attempt** — `lute scenario docs/examples/anseo`, `scenario reach`, `--format json`, plus
  a probe scene declaring itself downstream of a terminal.
- **Result** —
  ```
  project root: docs/examples/anseo
    topological layers:
      layer 0: scene(anseo.s01ep01)
      layer 1: scene(anseo.s01ep02)
      layer 2: scene(anseo.s01ep10), scene(anseo.s01ep11)
    edges (prerequisite -> dependent) [atom kind(s)]:
      scene(anseo.s01ep01) -> scene(anseo.s01ep02) [visited]
      scene(anseo.s01ep02) -> scene(anseo.s01ep10) [visited]
      scene(anseo.s01ep02) -> scene(anseo.s01ep11) [visited]
  ```
  ep10 and ep11 are leaves — but that is an `after:`-derived property and coincidence. The
  JSON node record is `{"id","kind","prereq","reach"}` and has no terminal field.
  `scenario reach anseo.s01ep10` reports `Reachable` and its prerequisites, nothing about
  what happens when you get there. Nothing distinguishes ep10 (terminates) from ep01 (does
  not); nothing flags a leaf that never terminates; the project checked clean through T1–T4
  with **zero** `::end` in it and no surface remarked on that either.
- **The probe that explains why.** A third scene, `after: 'visited("anseo.s01ep10")'` —
  declaring itself downstream of the scene whose only route ends in `::end`:
  ```
  ok: /tmp/t5probe/scenes/after-the-end.lute (0 warning(s))
      layer 3: scene(anseo.s01ep12)
      scene(anseo.s01ep10) -> scene(anseo.s01ep12) [visited]
  ```
  Clean, layered, `Reachable`. My first read was that this is a missing analysis. It is
  not — **it is correct, and it is correct because `::end` does not mean what its name says.**
  `directives.md` is precise about this: `::end` "is exactly equivalent to falling off the
  end of the command array, except that it carries a reason", and `lute-cli`'s own test is
  named `ending_matches_falling_off_the_end_except_for_the_reason` (identical `exit`,
  identical `state`). Every document falls off the end of its command array. So `::end` is
  a `break` with a label attached: it ends *this document's walk*, which ending the document
  does anyway, and it means nothing whatsoever about the run. `visited("anseo.s01ep10")`
  is satisfiable *because the scene was visited* — the walk stopped, the engine routes on.
  There is therefore no "does every route reach an ending" property to compute, because
  Lute has no ending to reach. `bridge.lute` with `::end` and `wake.lute` without it are,
  at the run level, the same kind of document.
- **What `::end` actually buys, precisely.** Two things, both real and both local: the
  free-form `reason` on one artifact record, and `W-CODE-AFTER-END` dead-code analysis
  within one straight-line body. Nothing else. It is well named for the first and
  mis-named for what an author reads into it.
- **Resolution** — `NONE — intent abandoned.` (i), (ii) and (iii) are unanswerable by any
  shipped tool, and (ii)/(iii) are not well-formed questions in the language's model.
- **Verdict** — `DOC-WRONG`, and located on one specific sentence rather than on the
  reference pages, which are accurate. The homepage's "Built for scenarios you can trust"
  card (`packages/website/src/content/docs/index.mdx:251-255`) reads:

  > Every scenario provably terminates — no infinite loops, no unbounded recursion — and
  > `::end` makes an ending explicit, so anything written after it is reported as dead
  > rather than quietly shipped.

  Both halves of the clause after the dash are false. `::end` does not make *an ending*
  explicit — it makes a document's early exit explicit, and `directives.md` says so two
  clicks away. And "anything written after it" is *not* "anything": it is the immediately
  enclosing straight-line body only, which `directives.md` also states correctly and this
  card contradicts. The load-bearing falsehood is the last four words — see T5.7, where the
  dead line is reported *and* quietly shipped, to the artifact, to the localization export,
  and to the production word count, at exit 0. This is the table's own argument for ranking
  `DOC-WRONG` above `DOC-GAP`: silence makes an author search, and the reference pages would
  have answered them. This sentence makes them stop searching, on the front page, in the
  section titled "scenarios you can trust". An author who reads it believes the language has
  endings and that dead content cannot reach the artifact; both beliefs are wrong and
  neither will be corrected by anything that fails.

#### T5.6 — a guard the checker honours in a scene is ignored in a quest predicate, and the diagnostic blames its absence — TOOL-DEFECT

Found reaching for T5.4(b)'s quest gate, on the un-defaulted ending enum.

- **Intent** — `fail="run.ending == 'shed-with-module'"` on a quest, where `run.ending` is a
  declared enum with no default (it has no honest default — before an ending, there is no
  ending). `E-MAYBE-UNSET`, correctly. `state-model.md` names the remedy: "a dominating
  `::set{p = …}` write **or a guard (`has(p)` / `isSet(p)`)** proves it".
- **Attempt** — apply the documented remedy in the only place a quest predicate has:
  ```lute
  <quest id="theWalk" … fail="isSet(run.ending) && run.ending == 'shed-with-module'">
  <objective id="reachBridge" … done="isSet(run.ending) && run.ending == 'bridge-reached'"/>
  ```
- **Result** — unchanged, and anchored on the guarded read:
  ```
  quests/the-walk.lute:8:74: error [E-MAYBE-UNSET] state path `run.ending` may be read before it is set (no default, no dominating `::set`, no guard) (dsl §9.4)
  quests/the-walk.lute:9:60: error [E-MAYBE-UNSET] state path `run.ending` may be read before it is set (no default, no dominating `::set`, no guard) (dsl §9.4)
  ```
  *"no guard"* — with `isSet(run.ending) &&` five characters to its left.
- **The narrowing exists; it is one construct away.** Three probes, same project, same path,
  same expression:
  | where | expression | result |
  |---|---|---|
  | scene content line `when=` | `isSet(run.ending) && run.ending == 'bridge-reached'` | `ok`, exit 0 |
  | quest `<objective done=>` | `isSet(run.ending)` alone | `ok`, exit 0 |
  | quest `<objective done=>` | `isSet(run.ending) && run.ending == 'bridge-reached'` | `E-MAYBE-UNSET` at col 35 |
  | quest `<quest fail=>` | `has(run.ending) && run.ending == '…'` | `E-MAYBE-UNSET` |
  So `isSet`/`has` are admitted in a quest predicate, and intra-expression `&&`
  short-circuit narrowing is implemented — for a scene line guard. The quest predicate slot
  does not run it.
- **Resolution** — added a sentinel `unspecified` member and `default: unspecified` to the
  enum. That works, and it is T5.4's cost item 3: a two-ending story declaring a
  three-member domain, and every exhaustive `<match>` over it carrying an arm for a
  non-ending, because the only other route to a quest gate on an optional path is closed.
- **Verdict** — `TOOL-DEFECT`, and it is the misdirecting-diagnostic case the protocol
  ranks near the top. The message does not say "a guard here must dominate the read" or
  "quest predicates are evaluated without flow context"; it says **"no guard"**, which is
  false about the text it is pointing at. An author who has just read `state-model.md`,
  applied the documented remedy, and been told the remedy is absent has no next move —
  the working fix (distort the domain with a sentinel) is not hinted at anywhere in the
  message, and the reason the fix is needed is invisible. Either arm closes it: run the
  same narrowing in the predicate slot, or say what is actually true in the message.

#### T5.7 — content after `::end` is reported *and* shipped: to the artifact, to `loc export`, and to the production word count — SPEC-WRONG

The assignment asks whether warning is the right severity for authored content that will
never play. Here is what the severity buys and what it costs, then my answer.

- **Attempt** — the required Step 3 probe. One content line after the shed terminator:
  ```lute
  @purser{code="0010" emotion="level" os}: Module released. Allocation is satisfied.
  ::end{reason="shed-with-module"}
  @vesna{code="0020" emotion="hollowed"}: Then we're the allocation.
  ```
- **Result** — the diagnostic is exemplary: one per body, at the first dead node, spec
  message verbatim, `--deny`-promotable (T5.1). And then, at exit 0:
  ```console
  $ lute check-project docs/examples/anseo          # ok, 5 file(s).  EXIT=0
  $ lute compile …/shed.lute -o /tmp/t5-dead.json
  {"kind":"line","addr":"001-0100","text":"Module released. Allocation is satisfied."}
  {"kind":"end","addr":"001-0200","reason":"shed-with-module"}
  {"kind":"line","addr":"001-0300","text":"Then we're the allocation."}   <-- proven dead

  $ lute loc export docs/examples/anseo -o /tmp/t5-loc.json
  1 lines untagged — run lute tag
  $ jq -r '..|objects|select(has("text"))|"\(.code)  \(.text)"' /tmp/t5-loc.json | grep -i allocation
  0010  Module released. Allocation is satisfied.
  0020  Then we're the allocation.                                        <-- for translation

  $ lute loc report docs/examples/anseo | grep shed
  docs/examples/anseo/scenes/shed.lute      2      9      …               <-- billed
  #  and with the probe line removed, the same row reads:  1      5
  ```
  The line the checker has *proven* unreachable becomes a command record with a real
  address, an entry in the localization export, and — by the difference between those two
  report rows — exactly 1 line / 4 words of the production budget. `loc export --help`
  calls itself "Extract **every translatable content line**".
  Money is spent translating and recording a line that cannot play.
- **The asymmetry.** This is one reachability pass with two severities. Its sibling verdict
  on provably-dead gated content is `E-ARM-DEAD` — an **error** — so that content never
  reaches an artifact, never reaches a translator, and never reaches a budget. Verified on
  a scratch scene, both forms, outside Anseo:
  ```console
  $ lute compile t5arm/b.lute -o /tmp/t5arm-b.json     # <choice … when="false">
  t5arm/b.lute:17:1: error [E-ARM-DEAD] choice can never fire: guard `false` is provably false (dsl 0.4 §5.2)
  1 error(s); no artifact emitted
  $ lute compile t5arm/a.lute -o /tmp/t5arm.json       # @narrator{when="false"}
  t5arm/a.lute:13:45: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard is provably false (dsl 0.4 §7.2, §5.2)
  1 error(s); no artifact emitted
  ```
  Post-`::end` content is the same proof of the same property in the same pass, and it
  ships. Nothing about the `::end` case is less certain: `W-CODE-AFTER-END` fires only on
  the provable straight-line case, which is why its scope is so carefully bounded (a
  sibling `<choice>`'s `::end` says nothing, and correctly does not warn).
- **My answer, with reasons.** Warning is the wrong severity; `E-CODE-AFTER-END` is right,
  and I would ship it as an error even though it is the more disruptive change.
  1. *The proof is total, not heuristic.* Every case this fires on is unreachable in the
     same sense `E-ARM-DEAD`'s is. Two severities for one proof needs a justification and
     0.8.0 §3 offers none.
  2. *Warning severity is load-bearing on the thing it fails to prevent.* A warning's
     contract is "this may be fine". Here the tool knows it is not fine, and the
     consequence is not stylistic: it is bytes in a shipped artifact and invoices in a
     localization pipeline.
  3. *`--deny` is not a mitigation.* Denial is per-project CI policy, chosen by whoever set
     up the build, and it promotes on a code the author of the dead line may never see. The
     default is what most projects get.
  4. *The counter-argument, and why it loses.* Dead content after a terminator is plausibly
     work-in-progress an author wants to keep while iterating — real, and the reason a
     warning was chosen. But that is also true of a dead `<branch>` arm, which is an error;
     comment it out, or move it above the `::end`. Iteration convenience does not outweigh
     shipping proven-dead content to paid downstream consumers, and the language already
     made that trade the other way one code over.
  5. *If it must stay a warning*, then the artifact and `loc` are the wrong place to pay
     for it: `compile` should drop provably-dead records, or `loc export` should skip them.
     Reporting *and* shipping is the one combination with no defensible reading.
- **Resolution** — probe line removed. `check-project docs/examples/anseo` back to
  `ok (5 file(s))`.
- **Verdict** — `SPEC-WRONG`. No implementation is at fault and the agreed design is the
  defect. 0.8.0 §3 specifies a warning; `directives.md` documents that warning and how to
  promote it; the checker emits exactly it, once per body, at the first dead node; and
  `compile` and `loc export` faithfully retain a record the language told them to keep.
  Language, docs, checker, compiler and localization all agree — which is why `DOC-GAP`,
  `DOC-WRONG`, `AUTHOR-ERROR` and `TOOL-DEFECT` are all false, and why
  `ERGONOMIC`/`LANGUAGE-GAP` do not apply (nothing here is about expressing anything). What
  the spec says is that provably-dead content after a terminator is a warning. What it
  should say is `E-CODE-AFTER-END`, an **error**, for the four reasons argued above (items
  1–4). This entry was filed as fitting no verdict and escalated; the seventh row exists
  for it.
  - **The strongest single fact, and it is in the checker's own source.**
    `W-CODE-AFTER-END` and `E-ARM-DEAD` are not two analyses that happen to agree about
    reachability — they are reached through the **same recursive reachability walk**.
    `crates/lute-check/src/reachability.rs` is one "§5.2/§5.3 whole-document reachability
    pass", and its `walk_reach` calls `check_code_after_end(nodes, diags)` on entry to
    every body it descends into, because "`nodes` is by construction exactly ONE
    straight-line body at every call site … so the `W-CODE-AFTER-END` scan rides this
    recursion instead of duplicating it". One walk, one PROVABLE-ONLY boundary, two
    severities — and the permissive branch is the one that ships bytes. Confirmed
    independently in review.
  - **Fallback, if compatibility forbids the error immediately.** Then `compile` and `loc`
    must at least **prune** the proven-dead content rather than shipping it: no addressed
    command record, no `loc export` entry, no `loc report` words. This is the reviewer's
    fallback position and item 5 above, reached separately. Reporting *and* shipping is the
    one combination with no defensible reading — and it is, word for word, what the front
    page promises cannot happen (T5.5).

  (T5.5 is `DOC-WRONG` rather than `SPEC-WRONG` because there the falsehood is in prose the
  reference pages already contradict — a wrong sentence, not a wrong decision. This entry
  has no wrong sentence anywhere.)

#### T5.8 — the `anchor` domain's declared `default:` cannot be written on purpose — ERGONOMIC

- **Intent** — Vesna at the helm, dead centre. The bridge is the one scene in the prologue
  where where she stands is the point, so the staging says so:
  `::auto{character="vesna" anchor="center" action="brace"}`. Anseo's `anchor` slot is
  `{ members: [port, center, starboard], default: center }`, declared in T1 long before this
  scene existed.
- **Result** — a permanent warning on the finished scene:
  ```
  docs/examples/anseo/scenes/bridge.lute:11:34: warning [W-INJECT-CONFLICT] `vesna` is shown with an explicit `anchor="center"` that `auto-anchor-on-show` would otherwise inject
  ```
  The message is accurate and the mechanism is right (no double injection, the author's
  anchor wins). But the *only* authored shape it fires on is agreement: writing a
  **different** anchor is honoured silently, and writing **none** is silent. So the one
  value an author cannot state explicitly is the one the schema calls the default — and
  `port`, which every other Anseo scene writes explicitly, is fine.
- **Resolution** — kept as written, warning and all. The three alternatives are all worse:
  delete a true statement about the staging; change `world`'s `anchor` `default:` to a
  member the project never uses, distorting the schema to silence a diagnostic; or omit the
  attribute and rely on an injection rule, which reads as an oversight to the next author.
  This is the first diagnostic in five tasks the corpus carries deliberately, so, to be
  unambiguous for Task 8 and for review: **`bridge.lute`'s warning is intentional and is
  the evidence for this entry, not an unfinished edit.**
- **Verdict** — `ERGONOMIC`, and slightly worse than it looks because there is **no
  suppression**. `lute check` has `--deny <CODE>` and `--deny-warnings` and no `--allow`,
  and there is no in-source acknowledgement — so a project on `--deny-warnings` in CI (which
  the toolchain's own docs encourage) cannot express "centre, on purpose" at all: it must
  either not say it or edit its vocabulary. `W-INJECT-CONFLICT` earns its keep in its other
  role — T2.1 cites it as the precedent for "this staging attribute is not doing what you
  think" — but redundancy and conflict are different claims, and this is the redundant case
  wearing the conflicting case's name and severity. A note-level severity, or an `--allow`,
  or simply not warning when the explicit value equals the injected one (nothing is lost,
  nothing is ambiguous, nothing is overridden) would each close it.

#### T5.9 — `lute trace` records the terminator and drops its only payload — ERGONOMIC

- **Intent** — preview the two endings in the author's preview tool, which is where the
  reasons should be most visible.
- **Result** — the walk stops in the right place and the terminator is recorded, but
  reasonlessly, in both renderings:
  ```console
  $ lute trace docs/examples/anseo/scenes/bridge.lute
    ## Shot 1.
      <auto>
      @vesna  Whatever's left of the ship, it's steering.
      <end>
  trace complete: 0 decisions
  $ lute trace …/bridge.lute --json | jq -c '.steps[]'
  {"kind":"directive","tag":"end","component_boundary":null}
  ```
  `<end>` is rendered exactly like `<auto>`, and the JSON `TraceReport` has no exit or
  disposition field at all (`["coverage","decisions","file","notes","seeds","steps","unresolved"]`),
  so a harness reading `trace --json` cannot tell a walk that was terminated from one that
  ran out of nodes, nor recover which ending it just previewed. `lute run` prints
  `end    reason=bridge-reached` from the same information.
- **Resolution** — used `lute run` on the compiled artifact to read the reasons back.
- **Verdict** — `ERGONOMIC`, deliberately not `TOOL-DEFECT`: `trace` records directive
  *tags* and not attributes, uniformly — `<auto>`'s `anchor`/`action` are dropped the same
  way — so this is a consistent terseness rather than a broken contract, and T3.12 records
  that `trace` renders branching honestly. The cost lands disproportionately on `::end`
  because `reason` is not one attribute among several: it is the terminator's *entire*
  payload, the only thing distinguishing it from falling off the end of the document
  (T5.5). A project with several endings previews them all as an identical `<end>`.

#### T5 summary

Nine entries: one *worked well* (T5.1), two `TOOL-DEFECT` — the vocabulary surfaces (T5.3)
and a misdirecting diagnostic (T5.6) — one `DOC-WRONG` (T5.5), three `ERGONOMIC` (T5.2,
T5.8, T5.9), one `LANGUAGE-GAP` (T5.4), and one `SPEC-WRONG` (T5.7). Every entry carries
exactly one of the seven verdicts; no `DOC-GAP` and no `AUTHOR-ERROR` scored here. T5 is
the first task to score either of the table's two newest readings, and both come from the
same place — what `::end` is and is not. The two claims a work with endings most wants to
make are not expressible, only proxyable (T5.4, shape (b)); and the one guarantee the
language does make about a terminator is specified at a severity that lets it be broken at
exit 0 (T5.7).

**`::end` works. It is also not what its name, or the front page, says it is.** The
construct itself is the cleanest first-use in this log: nine directives in `lute.core`, the
ninth exercised for the first time by this task, and it lowered, addressed, ran, and
dead-code-analysed correctly on the first attempt with no probing required (T5.1). What
five entries then measure is a gap between the construct and the concept an author brings
to it. `::end` is `break` plus a label: `directives.md` says it is "exactly equivalent to
falling off the end of the command array, except that it carries a reason", the CLI's own
test is named after that equivalence, and my after-the-terminator probe confirms it — a
scene may declare itself `after:` a scene that terminates, and that is *correct*, because
terminating a document's walk is what every document does. There is no ending in Lute. So
the three questions a branching work actually asks — which nodes are terminals, does every
route reach one, can a route dead-end without terminating — are not unanswered by the
tooling (T5.5); they are unaskable, and `lute scenario`'s node record has no field for
them because there is no property to put there.

**Two endings, one story is expressible only as two spellings of the same word, in
different languages, with nothing checking them against each other.** T5.4 is the entry to
read. The set claim and the polarity claim both resolve to the same workaround — mirror
each ending into a declared enum state path, `::set` it beside the `::end`, and let
`<match>` exhaustiveness (real, and good) or a quest `fail=` (real, typed, and observable
as `quest.…state = failed`) carry the structure. It works; it was driven end to end. It
costs a schema path the story does not need, a sentinel enum member that exists only
because a quest predicate ignores its own documented guard (T5.6), each ending said twice
with no cross-check, a `::set` half that is as untyped as the `reason` it mirrors (T3.2,
re-verified here for enum paths), a whole quest document to carry polarity, and an `end`
record that still tells a host nothing about whether the walk ended well. One optional
attribute on `::end` — a declared-domain `reason`, or an `outcome` — would collapse most
of that. That mirroring is a lossy proxy, not a wordier spelling — nothing in the language
means "these are the endings" or "this one is the failure", so nothing checks that the
proxy and the terminator beside it agree. That is why T5.4 is `LANGUAGE-GAP` shape (b).

**Two findings are about tools vouching for things that are not true, which is now the
dominant pattern of this log.** T5.3: declare `enums: reason: [...]`, and `lute context`
lists it in `projectEnums (8)` beside the seven enforced slots while the compiled artifact
ships it in the `enums` array documented as describing "the vocabulary it was compiled
against". It enforces nothing; any domain name behaves this way. That is T1.6 with the sign
flipped — `context` omitting what the project has, now inventing what it does not. T5.6:
`isSet(p) && p == x` narrows in a scene line guard, does not narrow in a quest predicate,
and the resulting `E-MAYBE-UNSET` says **"no guard"** while pointing five characters right
of one. Both are cheap fixes on information the checker already has.

**The one thing this task would change first is neither.** It is T5.7, the task's one
`SPEC-WRONG`: `W-CODE-AFTER-END` is a warning, so at exit 0 a line the checker has *proven*
unreachable becomes an addressed command record, an entry in `lute loc export` ("every
translatable content line"), and 4 words of `lute loc report`'s production budget — while
the same reachability pass's verdict on a dead `<branch>` arm is `E-ARM-DEAD`, an error,
which ships nothing. And it is not merely the same *kind* of proof: it is the same
recursive walk, one function call apart — `reachability.rs`'s `walk_reach` runs the
dead-code scan on entry to every body it recurses into rather than duplicating the
recursion. One walk, two severities, and the permissive one is the one that reaches a
translator's invoice. Reported *and* quietly shipped — which is, word for word, what the
homepage promises cannot happen (T5.5). `E-CODE-AFTER-END` is the fix; failing that,
`compile` and `loc` must prune what the checker has already proven dead.

Two housekeeping notes for whoever reads the corpus next. `bridge.lute` carries a
deliberate `W-INJECT-CONFLICT` (T5.8) — the `anchor` domain's declared `default:` is the
one member an author may not write on purpose, there is no `--allow` and no in-source
suppression, and the scene keeps the true statement rather than the clean output. And both
`after:` routes here are Task 8's to repoint; the graph in T5.5 is provisional by design.

One finding raised while probing T5.4's schema is deliberately **not** filed here:
`state-model.md`'s only `enum` state declaration example does not parse. It is outside
T5's remit (`::end`) and is **held for Task 10**, which owns the documentation gates; the
full reproduction is in `.superpowers/sdd/anseo/task-5-report.md`.

### T6 — The Purser component

Toolchain 0.9.0 / language 0.9.0 / IR 0.9.0, `./target/debug/lute`. One component added:
`components/purser-interject.component.lute`, the project's first `component:` document, and
`scenes/cryobank.lute`'s inline Purser line replaced by a `::use`. Components are the
language's only content-reuse mechanism, so this task is the first measurement of whether
Lute scales past one episode.

**Process note, recorded because the brief predicted it and it was true.** The binary at
`target/debug/lute` had mtime `14:05`; commit `3ff3543`, which closed the standalone leg of
the component-body contract, landed at `14:18`. The binary was stale. Rebuilding
`lute-cli` took 12s and every probe below was run against the rebuilt binary. A drive test
that had trusted the stale binary would have recorded the false green `3ff3543` fixed as a
live finding.

#### T6.1 — the component-body contract is enforced on both legs, and each diagnostic names its own rationale — WORKED WELL

- **Intent** — the Purser interjects in every module of the prologue at a different
  pressure. One block of words, one param, invoked eleven times. Then: establish that the
  presentational contract is real on both routes, because the whole value of a
  once-authored block is that the checker actually guards it.
- **Attempt** — the brief's Step 3 probe. A `<branch>` temporarily added after the
  component's `</match>`, then three checks: the component file alone, the importing
  scene, and `check-project`.
- **Result** — `E-COMPONENT-BODY` on every leg, exit 1:
  ```
  $ lute check docs/examples/anseo/components/purser-interject.component.lute
  …component.lute:28:1: error [E-COMPONENT-BODY] a component body must be presentational
    (dsl 0.4 §6.2): the `<branch probeOnly>` logic block is not allowed — presenting a menu
    records the selection, a state write; only a param-scoped `<match>` is admitted

  $ lute check docs/examples/anseo/scenes/cryobank.lute --project docs/examples/anseo
  …cryobank.lute:1:1: error [E-COMPONENT-BODY] component `purserInterject`
    (/…/purser-interject.component.lute): a component body must be presentational … (same tail)
  ```
  The same contract holds for state reads, with its own code and its own remedy, again on
  both legs — a `{{run.shedPressure}}` in a component body:
  ```
  error [E-COMPONENT-STATE] `run.shedPressure` reads ambient state — a component body may
    not depend on it; bind it through a param (dsl 0.4 §6.2)
  ```
  And the admitted exception works as specified: the param-scoped `<match on="@pressure">`
  with `<when is="rising">` / `<otherwise>` checks clean on both legs, and the `os` delivery
  flag survives the expansion (`"role": "offscreen"` in the artifact).
- **Resolution** — probe removed; the corpus checks clean at 6 files.
- **Verdict** — worked well, and worth stating in this register: each of the three
  messages says *why*, not just *no*. `E-COMPONENT-BODY` explains that a menu records a
  selection and that a selection is a state write — an author who reads it understands the
  rule rather than memorising a blacklist. `E-COMPONENT-STATE` names the remedy ("bind it
  through a param") in the message. This is the best-explained restriction measured in six
  tasks. It is also the restriction that turns out to matter least, for reasons T6.2 gives.

#### T6.2 — a component has no fixed meaning: one body, two callers, two different command streams, zero diagnostics — SPEC-WRONG

T6's structural finding. The vocabulary-scope limitation is documented as a *scoping*
inconvenience; it is a *semantic* one, and the difference is what a production pays.

- **Intent** — the Purser says the same thing, the same way, in every module. That is the
  entire reason to write a component instead of eleven lines. So: establish what "the same"
  is guaranteed to mean when two scenes with different vocabularies invoke the same
  component.
- **Attempt** — constructed in `/tmp/t6fix/proj`. Two vocabularies that agree on every member
  an author can see and on **every** other declaration, and disagree on exactly one thing an
  author cannot see — `action.exits`:
  ```yaml
  # vocabA.schema.yaml            # vocabB.schema.yaml
  emotion: [level, clipped]       emotion: [level, clipped]
  action:                         action:
    members: [brace, go-under]      members: [brace, go-under]
    exits: [go-under]               exits: [brace]
  anchor: {members: [port,center],  anchor: {members: [port,center],
           default: port}                    default: port}
  ```
  `diff vocabA.schema.yaml vocabB.schema.yaml` is one line, the `exits:` line. That control
  matters and cost a re-run: the first version of this probe also gave the two vocabularies
  different `anchor.default`s, which makes any causal claim about `exits` unsupported. It also
  hides a second caller-dependent divergence — with both defaults set to `center`, so that the
  component's explicit `anchor="center"` coincides with the injected default, caller B alone
  earns `W-INJECT-CONFLICT` ("`purser` is shown with an explicit `anchor="center"` that
  `auto-anchor-on-show` would otherwise inject") and caller A does not, because in A the sprite
  is an exit and nothing is *shown*. The transcript below sets both defaults to `port` so that
  interaction is out of the way and `exits` is the only live variable.

  One component, `uses: ../vocabA.schema.yaml`, body:
  ```lute
  ::auto{character="purser" anchor="center" action="go-under"}
  @purser{code="0010" emotion="clipped"}: The schedule advances.
  ```
  Two scenes, identical but for their `uses:`, each `::use{component="interject" pressure="rising"}`.
- **Result** — `ok: . (3 file(s), 0 project-wide warning(s))`, exit 0, and **no warning on any
  file** — not the component, not either caller. Nothing mentions that two callers disagree.
  And the two compiled artifacts are not the same artifact:
  ```console
  $ jq -c '.commands[]' outA.json        # caller A — TWO commands
  {"kind":"sprite","addr":"001-0100","character":"purser","anchor":"center","action":"go-under",
   "exit":true,"source":{"component":"interject"}}
  {"kind":"line","addr":"001-0200","role":"dialogue","speaker":"purser","text":"The schedule advances.",
   "emotion":"clipped","lineId":"probe.s01ep01.purser_0010","voiceKey":"purser-0010",
   "source":{"component":"interject"}}

  $ jq -c '.commands[]' outB.json        # caller B — THREE commands
  {"kind":"sprite","addr":"001-0100","character":"purser","anchor":"center","action":"go-under",
   "source":{"component":"interject"}}
  {"kind":"sprite","addr":"001-0200","character":"purser","preload":true,"emotion":"clipped",
   "provenance":{"injected":true,"by":"entry-emotion-lookahead",
     "reason":"pre-loading `purser`'s first emotion `clipped` seen ahead of the entrance"},
   "source":{"component":"interject"}}
  {"kind":"line","addr":"001-0300","role":"dialogue","speaker":"purser","text":"The schedule advances.",
   "emotion":"clipped","lineId":"probe.s01ep02.purser_0010","voiceKey":"purser-0010",
   "source":{"component":"interject"}}
  ```
  Both compiles exit 0. In caller A the Purser **leaves the scene** on that sprite
  (`exit: true`) and then speaks a line after having left (T2.4's defect, arriving here through
  a component whose author wrote no exit at all). In caller B the same line is an **entrance**,
  so the `entry-emotion-lookahead` rule fires and injects a whole extra command that exists in
  neither the component nor the scene. Two commands versus three, different addresses for
  the same authored line, opposite staging semantics. One body, one differing enum attribute
  that no author can see from the callsite. No diagnostic.
- **Resolution** — for Anseo, the mitigation the docs prescribe: `purser-interject.component.lute`
  declares `uses: ../vocabulary.schema.yaml` and every caller reaches the *same* project-root
  schema, so divergence is impossible by construction. That is a discipline, not a guarantee —
  nothing checks it, and the next author to give one scene its own `action` domain gets the
  transcript above at exit 0.
- **Verdict** — `SPEC-WRONG`. No implementation is at fault and the docs are not silent:
  `vocabulary.md` has a section headed "Known limitation: a component body resolves against
  the importing document", it opens "State it plainly, because it will bite someone", it
  says a component's own `uses:` and inline `enums:` "are both discarded at parse", and it
  names the future direction ("A *component schema* surface … is a named future direction
  filed separately"). Everything works exactly as specified. **The specification is the
  defect**, and specifically the sentence that frames it: *"This is a scoping limit, not a
  checking divergence."* That is true about *checking* and false about *meaning*. The
  discarded `uses:` does not merely fail to bring vocabulary along — it means the language's
  only reuse construct has **no denotation of its own**. `interject` is not a block of
  content; it is a block of content *per caller*, and the callers can disagree about whether
  it removes a character from the stage.

  Two alternatives, either sufficient, both cheap. (i) The named future direction: `::use`
  carries the component's declared domains into the expansion, so the component means what
  its author wrote. (ii) If that is too large for a point release, the *detection* is nearly
  free, because `check-project` already resolves every component and every caller in one
  run: for each slot a component body touches, compare the component's own declared domain
  against each caller's and emit `W-COMPONENT-VOCAB-DIVERGENT` when they differ. That would
  have fired twice on the transcript above. As shipped, the only reuse mechanism in the
  language is the one construct whose behaviour nothing in the project can pin down, and the
  doc that warns you about it under-describes what it costs.

#### T6.3 — no check is both caller-aware and able to point into the component: the fault is reported N times, in the N files that are correct, while the one file to edit reports `ok` — TOOL-DEFECT

- **Intent** — a component is authored once and validated per caller. Ask the practical
  question directly: if it is correct against caller A and broken against caller B, when and
  where do I find out?
- **Attempt** — same `/tmp/t6/proj`, now with the component pointed at `vocabB` and using a
  member only `vocabB` declares (`emotion="molten"`), with seven callers of various depths
  in the project.
- **Result** — the fault **is** detected, and `check-project` **is** the command that detects
  it: exit 1, `failed: .`. State that first, because it bounds the finding — this is not a
  hole in coverage, it is a hole in *localisation*. What no command gives you is a check that
  is simultaneously caller-context-aware and able to point at the component's own source span.
  The two legs split those properties between them and neither has both.

  The component's own check **passes**, with and without `--project`:
  ```console
  $ lute check components/interject.component.lute                  # ok, exit 0
  $ lute check components/interject.component.lute --project /tmp/t6/proj   # ok, exit 0
  ```
  `check-project` catches it, and reports it once per caller, at line 1 of the wrong file:
  ```console
  $ lute check-project .                                            # exit 1
  scenes/sceneA.lute:1:1:  error [E-BAD-ENUM] component `interject` (/…/interject.component.lute):
    `molten` is not a valid value for `emotion` of `::purser` (expected one of: level, clipped)
  scenes/nest.lute:1:1:    error [E-BAD-ENUM] component `interject` (…) : (identical)
  scenes/callsite.lute:1:1: error [E-BAD-ENUM] component `interject` (…) : (identical)
  components/outer.component.lute:1:1: error [E-BAD-ENUM] component `interject` (…) : (identical)
  ok: components/interject.component.lute (0 warning(s))
  failed: . (… 0 project-wide error(s), 0 project-wide warning(s))
  ```
  So the file the author must edit is the one file reporting `ok`, and the N files reporting
  errors are the N files that are correct. With eleven modules that is eleven identical
  messages, all at `1:1`, none of them in the component. The caller-context-awareness is real
  and worth crediting — `sceneB.lute`, whose vocabulary *does* declare `molten`, correctly
  reports `ok` in the same run. It is the position that is thrown away.

  **And the position exists.** The standalone leg printed `28:1` for T6.1's `<branch>`, and in
  the same `check-project` run above it prints `reader.component.lute:9:54` for an
  `E-COMPONENT-STATE` inside a component body — a component-internal span, on the standalone
  leg, in the caller-aware command. The caller leg prints `1:1` for a fault it has located
  precisely enough to quote the offending value and enumerate the legal alternatives. The
  checker knows the span inside the component body and does not pass it through on the one
  fault class that needs it. There is also no way to ask the question the author actually has
  — "check this component as caller A would" — because `--project` does not change the
  component's resolution root (proven above): a standalone check resolves against the
  component's own `uses:`, which is the one vocabulary that is *never* the one that applies at
  runtime.
- **Resolution** — none available. The working procedure is: never trust `lute check` on a
  component file, always `check-project`, and read the component path out of the message
  prefix rather than the file position.
- **Verdict** — `TOOL-DEFECT`, on the protocol's highest-priority ground — a diagnostic that
  misdirects. Not "a component cannot be validated": it can, by `check-project`, which returns
  failure. It is a *double misdirection about where*, and the pairing is what makes it
  expensive: `lute check <component>` reports `ok` on a component that cannot work (not a
  false green — it is true against the component's declared vocabulary — but a *meaningless*
  green, since that vocabulary governs nothing), while the caller-side error points at the
  caller's frontmatter for a fault that is N files away. The fix is two small ones on
  information the checker already holds: forward the component-internal span into the prefixed
  diagnostic (`…/interject.component.lute:10:34`, reported at the caller), and roll the N
  identical caller reports into one per *problem* the way T1.3 praises `E-UNDECLARED` for doing
  (`(+6 more callers)`).

#### T6.4 — `::set` is forbidden in a component body, and the rule is right — WORKED WELL

Recorded as a vindication, because a maturity report that never vindicates a restriction is
not an assessment.

- **Intent** — a Purser interjection that *costs power*. Reading the crew's draw is the
  Purser's whole function in this story, and the beat is "it notices, and the schedule
  advances" — one command, one price. So the natural first form puts the price in the
  component, beside the words:
  ```lute
  ## Costly
  ::set{run.shedPressure += 1}
  @purser{code="0010" emotion="level"}: Allocation notes the draw.
  ```
- **Result** — refused, on both legs, with the rationale in the message:
  ```
  …costly.component.lute:9:1: error [E-COMPONENT-BODY] a component body must be presentational
    (dsl 0.4 §6.2): `::set` of `run.shedPressure` writes state — only a param-scoped `<match>`
    is admitted for logic, not a state write
  …costly.component.lute:9:7: error [E-UNDECLARED] state path `run.shedPressure` is not declared
    in `state:` (dsl §9.4)
  ```
- **Resolution** — the price moved to the callsite, which checks clean and is what the
  committed corpus does in spirit (cryobank's arms carry their own `::set`s):
  ```lute
  ::set{run.shedPressure += 1}
  ::use{component="interject" pressure="rising"}
  ```
- **Verdict** — worked well; the rule serves the author here, and I would not change it. The
  reason is not purity, it is legibility under reuse. A component that writes state is an
  invisible `+= 1` fired eleven times from eleven files that do not say so; the one number
  the whole Anseo prologue is about would become impossible to audit by reading. Putting the
  cost at the callsite makes the price legible exactly where the beat is priced, and it lets
  different modules charge different amounts for the same words — which is what a real
  production wants anyway. The restriction is also *consistent*: T6.1's `E-COMPONENT-STATE`
  blocks the read for the same reason, and "bind it through a param" is genuinely the right
  answer.

  Two honest caveats, neither of which changes the verdict. **The pairing is unenforced.**
  Nothing in the language says `purserInterject` must be accompanied by a pressure
  increment; the component guarantees the words and guarantees nothing about the cost, so
  the tenth module that forgets the `::set` is silent. **And the second diagnostic is
  noise** — `E-UNDECLARED` on `run.shedPressure` fires because a component has no state
  schema in scope and never can, so it is a guaranteed companion error on every occurrence
  of the first, telling the author to declare a path they are forbidden to write.
  Suppressing state-path resolution once `E-COMPONENT-BODY` has fired on the same directive
  would leave a clean single message.

#### T6.5 — reuse with variation: nesting, param threading, and params inside `<match>` arms all work — WORKED WELL

- **Intent** — the things a real production reaches for second. The Purser speaks in every
  module at a different pressure; the mechanism is the param-scoped `<match>`. Push it:
  a component invoking another component, a param threaded through that invocation, and arm
  content that is itself parameterised.
- **Result** — all three work, first form reached for.
  - **Component invoking a component.** A component file accepts `components:` in its own
    frontmatter and `::use` in its body, and a param threads through the inner invocation:
    ```lute
    ---
    component: outer
    params: { pressure: string }
    components: [interject.component.lute]
    ---
    ## Outer
    ::use{component="interject" pressure=@pressure}
    ```
    `ok` standalone and through a scene, and the expansion compiles correctly. `lute trace`
    renders the nesting honestly — two `-- component begin --` markers, correctly nested.
  - **Parameterised arm content.** A `@param` in an attribute position inside a `<when>` arm
    is accepted: `::auto{character=@who anchor="port" action="brace"}` inside
    `<when is="rising">` checks clean. So arms are not second-class; the whole param surface
    is available inside them.
  - **The `<match>` fold is the well-behaved case of T5.7's machinery.** cryobank passes the
    literal `pressure="rising"`, so the `<otherwise>` arm is statically unreachable *for this
    caller*. All three tools do the right thing and agree: the artifact **prunes** it
    (`grep -c "Allocation is nominal"` over `commands[]` → `0`), `check-project` emits **no**
    dead-arm diagnostic (correct — another caller may pass something else), and `loc export`
    **does** carry it (correct — another caller may need it translated). That is precisely the
    combination T5.7 faults `W-CODE-AFTER-END` for getting backwards, reached here by the
    same reachability pass. Worth saying plainly: the fold is right.
- **Verdict** — worked well. Nothing this beat wanted from the variation surface was missing,
  and the nesting in particular is the thing most likely to be a stub in a young language.

#### T6.6 — a component param cannot have a default, and the long form every other schema surface uses is rejected — ERGONOMIC

- **Intent** — the Purser is nominal in most modules and rising in a few. So the natural
  declaration gives the common case a default and lets nine of eleven callers write
  `::use{component="purserInterject"}`.
- **Attempt** — `params: pressure: { type: string, default: "steady" }`, then a bare `::use`.
- **Result** — the accepted param grammar is exactly two forms, established by narrowing all
  five candidates through a caller:
  | form | result |
  |---|---|
  | `pressure: string` | accepted |
  | `pressure: { enum: [steady, rising] }` | accepted |
  | `pressure: { type: string }` | `E-COMPONENT-PARSE` |
  | `pressure: { type: string, default: "steady" }` | `E-COMPONENT-PARSE` |
  | `pressure: { enum: [steady, rising], default: steady }` | `E-COMPONENT-PARSE` |
  ```
  error [E-COMPONENT-PARSE] component file `/…/pf.component.lute` has a malformed `params:`
    — each entry must be `name: <type>` (dsl §13)
  ```
  Omitting an arg is clean and well-aimed: `error [E-COMPONENT-ARG] component `pf` requires
  argument `pressure` (dsl §13)`, at the `::use` line and column.
- **Resolution** — every caller spells every argument. For Anseo that is one arg on one
  callsite; for the eleven-module version of this component it is eleven copies of
  `pressure="nominal"`, which is the verbosity a component exists to remove.
- **Verdict** — `ERGONOMIC`, with two distinct costs. **No defaults** is the smaller one and
  arguably defensible — an explicit arg at every callsite is legible, and `E-COMPONENT-ARG`
  makes the omission a check-time error rather than a silent fallback. **The rejected
  `{ type: … }` is the sharper one**, because it is inconsistent rather than restrictive: the
  long form is how `state:` entries are written, how `defs:` entries are written
  (`warm: { type: bool, cel: … }`), and how the `anchor` domain is written in Anseo's own
  vocabulary — and `components-and-extends.md` says component params are "typed exactly like
  a [def param]". An author who has read the rest of the YAML surface writes `{ type: string }`
  and is told their `params:` is malformed. Accepting `{ type: X }` as a synonym for `X`
  would cost nothing and close it.

#### T6.7 — the component file's own check blames the reference for a fault in its own frontmatter, while the caller's check names the cause — TOOL-DEFECT

Split out of T6.6, where it was found, because it is a misdirection and the protocol ranks
those above the ergonomics of the thing that produced it.

- **Attempt** — the same malformed `params:` from T6.6, checked from both legs.
- **Result** — the two legs disagree about what is wrong, and the leg that owns the file
  gets it wrong:
  ```console
  $ lute check components/defaulted.component.lute --project /tmp/t6/proj
  …defaulted.component.lute:9:51: error [E-UNDECLARED-REF] `@pressure` is not a declared def (dsl §8.1)

  $ lute check scenes/probes.lute --project /tmp/t6/proj
  …probes.lute:1:1: error [E-COMPONENT-PARSE] component file `/…/defaulted.component.lute` has a
    malformed `params:` — each entry must be `name: <type>` (dsl §13)
  …probes.lute:1:1: error [E-UNDECLARED-REF] component `defaulted` (…): `@pressure` is not a
    declared def (dsl §8.1)
  ```
  The component's `params:` failed to parse, so the param was never registered, so the body's
  `@pressure` resolves against nothing. The caller reports **both** the cause and the
  consequence, in that order. The component's own check reports **only the consequence** — and
  reports it as `E-UNDECLARED-REF … not a declared def`, which sends the author to
  `defs:`/§8.1 for a param they declared four lines up and one character wrong.
- **Control, added during review at `AnseoT6Rev`'s request.** The body above puts the param in
  a `{{@pressure}}` interpolation, which T6.8 shows is independently illegal for a `string`,
  so the split could have been an artefact of that position. It is not. Same malformed
  `params:` with the param in an **attribute** position and no interpolation anywhere —
  `params: who: { type: string, default: "purser" }`, body `::auto{character=@who …}` —
  reproduces it exactly: standalone gives only
  `attrpos.component.lute:9:18: error [E-UNDECLARED-REF] `@who` is not a declared def`, and the
  caller gives `E-COMPONENT-PARSE` **and** the prefixed `E-UNDECLARED-REF`. The misdirection is
  a property of the leg, not of the ref position. Minimal single-component isolate:
  `/tmp/t6/t67`.
- **Resolution** — read the error from the caller, not from the file that contains it.
- **Verdict** — `TOOL-DEFECT`. The information exists — the same binary prints
  `E-COMPONENT-PARSE` for this exact file, from the other leg, one command later — and the
  tool nearest the fault does not hand it over. This is the T6.3 pattern in miniature and it
  compounds with it: the standalone leg is the only one that can point *into* a component
  file, and on the two faults measured here it either says `ok` (T6.3) or blames the wrong
  construct (this entry). `E-COMPONENT-PARSE` should fire on the standalone leg too, and
  should suppress the downstream `E-UNDECLARED-REF` it causes.

#### T6.8 — `{{@param}}` cannot render a `string`, the doc says it can, and the one built-in interpolation *is* a string — DOC-WRONG

- **Intent** — reuse with variation, in the place variation is most wanted: the words. The
  Purser names the module it is billing — "Draw exceeds projection in {{@module}}" — so one
  component carries eleven interjections instead of eleven near-duplicate blocks.
- **Attempt** — the form the components page documents, verbatim from its own worked example's
  param type:
  ```lute
  params:
    who: string
  ---
  @purser{code="0010" emotion="level"}: {{@who}}, the schedule advances.
  ```
- **Result** —
  ```
  …interp.component.lute:9:39: error [E-REF-TYPE] `@who` produces a non-renderable type;
    a `{{…}}` interpolation renders only number/bool/enum (dsl §7.6)
  ```
  Narrowed across all four param types, one component each: `number` `ok`, `bool` `ok`,
  `{ enum: [low, high] }` `ok`, **`string` errors**. So the only param type that carries
  arbitrary text is the only one that cannot be interpolated into text — and it is the type
  both shipped component examples declare (`greet`'s `who: string`, and this task's
  `pressure: string`).
- **Resolution** — `NONE — intent abandoned`. The component varies its words by
  `<match>`-ing on the param and writing each variant out in full, which is what
  `purser-interject.component.lute` ships. That works for two variants and does not scale to
  eleven module names; for those the words would go back to being authored per scene, i.e.
  reuse abandoned for exactly the beat that wanted it.
- **Verdict** — `DOC-WRONG`. `components-and-extends.md` states it flat and unqualified:
  *"A parameter is referenced as `@<param>` in ref and attribute positions, and inside content
  text via `{{@param}}` interpolation."* One sentence later it explains that `@who` is legal
  in the `character` position "only because that attribute is `string`-typed" — so the page
  is careful about type restrictions in the attribute position and silent about the one in
  the interpolation position, immediately above an example whose only param is a `string`.
  Nothing on the shipped website states the renderable-type rule anywhere: `grep` for
  "renderable" / "E-REF-TYPE" / "number/bool/enum" across `packages/website/src/content/docs`
  returns one hit, `params.md:77`, which says only that a whole-slot `@ref` must "produce the
  position's required type" and never names the interpolation whitelist.
  `dialogue-and-cast.md`'s interpolation section says an interpolation "must name a
  **declared** state path" and says nothing about type. So the author is told it works, at
  the type the example uses, and finds out from a diagnostic.

  **The doc fix is not the whole answer, and the rest is now filed separately.** The
  restriction this page fails to document is itself defective — the runtime renders strings,
  and the language forbids the only param type that carries one. That is a different verdict
  against a different artifact (the spec, not the page), so it is **T6.11**, filed as
  `SPEC-WRONG` at the controller's direction rather than hyphenated onto this entry. This
  entry's verdict stands alone: whatever §7.6 ought to say, `components-and-extends.md` states
  the opposite of what §7.6 says *today*, one line above an example that cannot compile.

#### T6.9 — provenance is carried on every surface a consumer reads, and the human renderings drop the name — WORKED WELL

- **Intent** — read cryobank back as a consumer, and as a translator. Can either tell which
  lines came from a component and which were authored inline?
- **Result** — yes, on all four surfaces, which is better than this log's usual finding about
  artifact fidelity (T4.8, T5.9):
  - **compiled artifact** — every expanded command carries `"source": {"component": "purserInterject"}`,
    and inline commands carry no `source` at all. Unambiguous, per command, machine-readable.
  - **`lute trace`** — `-- component begin --` / `-- component end --` around the expansion,
    correctly nested for a component invoking a component.
  - **`lute trace --json`** — `{"kind":"directive","tag":"__component-begin","component_boundary":"begin"}`,
    so a harness can bracket the region.
  - **`lute context --json`** — `components: [{"name":"purserInterject","params":[{"name":"pressure","type":"string"}]}]`.
    Name *and* param names *and* types. After T1.6 and T3.7 this is worth crediting
    explicitly: on components, `context` ships the grammar an author needs to write the
    `::use`, not just the identifier.
- **Verdict** — worked well. Three narrow gaps, none of which changes that:
  1. **The human renderings drop the name.** `lute context`'s outline prints
     `components (1): purserInterject` with no params — a harness reading the human form
     cannot write the `::use`; the `--json` form is complete. And `trace`'s
     `-- component begin --` carries no name, so a scene invoking three components previews
     three identical markers.
  2. **Nesting collapses to the innermost name.** `outer` → `interject` produces
     `"source":{"component":"interject"}` on every command, byte-identical to a direct
     invocation of `interject`. The chain is visible in `trace` (as depth) and lost in the
     artifact (as identity).
  3. **`__component-begin` is a leaked internal.** A double-underscore synthetic tag in the
     surface T5.9 establishes harnesses read; `component_boundary` beside it already carries
     the meaning.

#### T6.10 — every component line is silently dropped from the localization bundle, and the remedy the tool names is a no-op — TOOL-DEFECT

T6's most serious finding, and the one that would stop a real production from using
components at all. Adopting the language's only reuse mechanism *removed a line from the
localization pipeline*, and the before/after is one command.

- **Intent** — the translator question. The Purser says this in eleven modules; find out what
  `lute loc` hands a translator, and specifically whether they see the line once or eleven
  times.
- **Attempt** — the full round trip on the committed corpus: `loc export` → translate every
  row → `loc import` → `compile --locales`.
- **Result — the good half, and it is genuinely good.** `loc export` emits the component's
  lines **once**, keyed to the component file and its real line number, not once per caller:
  ```json
  { "code": "0020", "file": "docs/examples/anseo/components/purser-interject.component.lute",
    "kind": "line", "line": 21, "lineId": null, "speaker": "purser",
    "text": "Draw exceeds projection. The schedule advances." }
  ```
  `loc report` agrees and counts it once, as its own document — 2 lines / 9 words, `tagged 2`.
  A translator does *not* see the same line eleven times, and a producer does not pay for it
  eleven times. That is exactly right, and it is the strongest single argument for components
  in this whole task.
- **Result — `lineId` is `null`, and everything downstream is keyed on `lineId`.**
  ```console
  $ lute loc import /tmp/t6/ja-JP.json -o bundle.json
  3 rows skipped (no lineId) — run lute tag, then re-export
  exit=0

  $ lute tag docs/examples/anseo/components/purser-interject.component.lute
  lute: already tagged                        # exit 0, file unchanged (diff: no change)

  $ lute compile …/cryobank.lute --project docs/examples/anseo --locales bundle.json
  …cryobank.lute:1:1: warning [W-L10N-MISSING] no `ja-JP` text for `anseo.s01ep02.purser_0020`
  exit=0
  $ jq -r '.commands[]|select(.kind=="line")|"\(.lineId)\t\(.text)"' cryo-ja.json
  anseo.s01ep02.vesna_0010   [ja] Every pod you crack, the Purser reads as load.
  anseo.s01ep02.purser_0020  Draw exceeds projection. The schedule advances.   ← SOURCE LANGUAGE
  ```
  The chain: the row exports with no `lineId`; `import` skips it and exits **0**; `compile
  --locales` demands `anseo.s01ep02.purser_0020` — a **caller-derived** id that no export row
  ever carried — and ships the untranslated string at exit 0.

  **The named remedy cannot work.** `loc import --help` documents the bundle as keyed on
  `lineId` ("a duplicate `lineId` within one locale" is its error case), and `lute tag`
  "back-fills a stable `code` into every untagged `:line`". These lines are not untagged —
  they carry `code="0020"` / `code="0010"`, `loc report` counts them as `tagged 2`, and `lute
  tag` answers `already tagged` and changes nothing. Their `lineId` is null because
  `{prefix}` derives from `{character}.s{season}ep{episode}` in the *importing* document's
  frontmatter, which a component does not have and structurally cannot have. No amount of
  `lute tag` will ever produce one. The message sends the author to a command that is a
  guaranteed no-op, at exit 0.
- **Result — the before/after, which is the measurement.** The same Purser beat, one commit
  apart, moved from inline to `::use`:
  ```console
  $ lute loc export <HEAD, inline>  | jq -r '.[]|select(.speaker=="purser")|…'
  anseo.s01ep02.purser_0020   cryobank.lute      Allocation notes the draw. The schedule advances.
  anseo.s01ep11.purser_0010   shed.lute          Module released. Allocation is satisfied.

  $ lute loc export <after, ::use> | jq -r '.[]|select(.speaker=="purser")|…'
  NULL                        purser-interject.component.lute   Draw exceeds projection. …
  NULL                        purser-interject.component.lute   Allocation is nominal.
  anseo.s01ep11.purser_0010   shed.lute          Module released. Allocation is satisfied.
  ```
  Inline, the line was localizable. Through the language's reuse mechanism, it is not.
  Anseo's null-`lineId` count went 1 → 3 on this task, and the one pre-existing null is a
  genuinely untagged quest line that `lute tag` *can* fix — so the message is correct for one
  of three rows and impossible for the other two.
- **Resolution** — none available, and the corpus ships the defect. `check-project` is `ok`
  at 6 files; `compile --locales` is the only command that says anything, at warning severity,
  naming an id the author will not find in any export they were given. **Kept as written**,
  because the alternative is to abandon components for any translated line, which is the
  finding.
- **Verdict** — `TOOL-DEFECT`, and deliberately not `LANGUAGE-GAP`. The *language* is right:
  a component line's identity is caller-derived, which is the correct semantics — one source
  of words, eleven addressable lines. Every failure is in a tool, and the information all
  three need is present in a single `check-project`/`compile` run: `loc export` knows the
  callers (it is a project-wide walk), `compile` knows all eleven `lineId`s, and `loc import`
  knows the row's `file` is a component. Three concrete fixes, any one of which unblocks a
  translated production: (i) `loc export` emits one row **per expansion**, carrying the
  caller-derived `lineId`, with the component file+line retained as a `source` field so a TMS
  dedupes on identical source text and a translator still sees the string once — this is the
  right fix, because it also makes `W-L10N-MISSING` unreachable; (ii) failing that, the bundle
  accepts a component-scoped key (`purserInterject#0020`) that `compile --locales` resolves
  before falling back to `lineId`; (iii) at minimum, `loc import` must not name `lute tag`
  for a row whose file declares `component:`, and skipping a translated row should not be
  exit 0 — `loc export`'s own "1 lines untagged — run lute tag" precedent shows the surface
  for saying so honestly.

#### T6.11 — the interpolation whitelist forbids the one param type that carries text, while the runtime renders text on the other two interpolation forms — SPEC-WRONG

Escalated out of T6.8 at the controller's direction. T6.8 is the *page* being wrong about
what §7.6 says; this is §7.6 itself being the wrong rule. Filed against the language because
the drive test's remit **is** the language.

- **Intent** — the same beat T6.8 wanted: one component whose words vary by an author-supplied
  string — "Draw exceeds projection in {{@module}}" — so eleven modules share one block
  instead of eleven near-duplicates. Then, separately: establish whether the restriction that
  blocks it is a coherent rule or an accident of which grammar alternative you land in.
- **Attempt** — three probes in `/tmp/t6fix/proj`, all against the freshly built binary.
  (a) One component per param type, each body a single line reading `{{@who}}, the schedule
  advances.`; (b) the same line with the reserved token `{{userName}}` instead; (c) the same
  line reading a `string`-typed **declared state path**, `{{run.label}}`, declared
  `run.label: { type: string, default: "nominal" }`.
- **Result** — the whitelist binds exactly one of the three interpolation forms, and it is the
  one a component param uses.
  ```console
  $ lute check components/ip-number.component.lute        # ok         (params: who: number)
  $ lute check components/ip-bool.component.lute          # ok         (params: who: bool)
  $ lute check components/ip-enumlowhigh.component.lute   # ok         (params: who: { enum: [low, high] })
  $ lute check components/ip-string.component.lute        # exit 1     (params: who: string)
  components/ip-string.component.lute:9:39: error [E-REF-TYPE] `@who` produces a
    non-renderable type; a `{{…}}` interpolation renders only number/bool/enum (dsl §7.6)
  ```
  The reserved token — a string — checks clean and compiles to a placeholder the runtime
  substitutes:
  ```console
  $ lute check components/ip-user.component.lute          # ok, exit 0
  $ jq -c '.commands[]' outUser.json
  {"kind":"line",…,"text":"{{userName}}, the schedule advances.",…,
   "placeholders":[{"kind":"reserved","token":"userName"}],"source":{"component":"ipuser"}}
  ```
  And so does a `string`-typed declared state path, which is the probe that settles it:
  ```console
  $ lute check scenes/strpath.lute --project .            # ok, exit 0
  $ jq -c '.commands[]' outStr.json
  {"kind":"line",…,"text":"Draw stands at {{run.label}}.",…,
   "placeholders":[{"kind":"path","path":"run.label"}]}
  ```
  So: `{{userName}}` renders a string. `{{run.label}}` renders a string. `{{@who}}` is a
  static error *because* it is a string. Three forms of one construct, one shared runtime
  substitution mechanism (`docs/runtime/state-lifecycle.md`: the `placeholders` list names each
  referent — "a state `path`, an `@`-`ref`, or a `reserved` token" — and "the engine substitutes
  these against live state"), and the type rule applies to exactly one of them.
- **Resolution** — `NONE — intent abandoned`, same as T6.8: the shipped component varies its
  words by writing each variant out in full under a `<match>`, which does not scale past a
  handful and does not reach eleven module names at all.
- **Verdict** — `SPEC-WRONG`. Nothing here is misimplemented and, unlike T6.8, nothing here is
  a contradiction on the page either — that distinction matters and I had it wrong first time.

  **What the spec says.** `docs/proposals/scenario-dsl/0.1.0.md` §7.6 gives three grammar
  alternatives — `Interp ::= "{{" ( Path | Ref | ReservedToken ) "}}"` — and attaches the type
  rule to precisely one: *"An interpolated `Ref` MUST resolve to a renderable type (number /
  bool / enum, per the rendering rule below); a `@def` of any other type is a static error."*
  `ReservedToken` gets its own bullet with no type rule at all (*"`userName` renders the
  runtime player name"*), and the normative **Rendering** paragraph enumerates only
  number/bool/enum. So `{{userName}}` is not a violation of the whitelist; it is *outside* it,
  by explicit construction. **This is not a literal grammar contradiction** and should not be
  reported as one. The `Path` case is looser still — §7.6's rendering paragraph never defines
  how a string path renders, and the checker admits it silently.

  **Why that is the wrong call.** The defect is a **capability mismatch**, not an
  inconsistency. The rendering pipeline demonstrably renders arbitrary strings: it does not
  interpolate at compile time at all, it emits a `placeholders` list and keeps the raw text,
  and the engine substitutes at present time. Nothing about a `string` is unrenderable — two
  of the three interpolation forms ship strings today. What §7.6 actually does is forbid the
  **only param type that can carry arbitrary author text** from reaching the one position that
  displays text, in the language's **only** content-reuse construct. The cost is not a
  workaround; it is that varying a component's words by an argument is unreachable, which
  removes the main reason to write a component with a param at all — and the two shipped
  component examples both declare `string` params (`greet`'s `who`, this task's `pressure`),
  so the spec forbids the shape its own documentation models. If the rule's motive is
  localization safety — splicing untranslated author text into a `lineId`-keyed line is a real
  hazard, and §7.6's `E-L10N-PLACEHOLDER` placeholder-set contract is the surface that would
  police it — then the rule as written does not achieve it, because `{{run.label}}` splices an
  arbitrary string into a translatable line at exit 0 and `{{userName}}` is taught on
  getting-started page one.

  **What it should say instead.** Replace the produced-type whitelist with a
  substitution-mechanism rule, which is what the implementation already is:

  1. **Admit `string` as a renderable type** and extend the normative Rendering paragraph to
     cover it (*"a **string** → its text verbatim"*), making the sentence true of all three
     forms rather than one. `E-REF-TYPE` then keeps its real job — a `@def` producing a
     *structural* type (map/list) is still a static error, which is the case the rule was
     presumably written for.
  2. **Keep the safety story explicit** by scoping it where it belongs: every interpolation,
     of any form, is already a placeholder in the line's translatable text, and
     `E-L10N-PLACEHOLDER` already enforces placeholder-set equality across translations. If a
     project wants to forbid interpolating unbounded text into translatable lines, that is a
     lint over *all* placeholder kinds (`W-INTERP-FREE-TEXT`, say, which would fire on
     `{{run.label}}` too), not a type ban that one grammar alternative happens to escape.
  3. **Failing both**, if the ban is deliberate and permanent, then say so *as a rule about
     component params* and make the diagnostic say it — `E-REF-TYPE`'s current text
     ("a `{{…}}` interpolation renders only number/bool/enum") is a claim about interpolation
     that the same binary falsifies twice in the transcript above.

#### T6 summary

Eleven entries: four *worked well* (T6.1, T6.4, T6.5, T6.9), three `TOOL-DEFECT` (T6.3, T6.7,
T6.10), two `SPEC-WRONG` (T6.2, T6.11), one `DOC-WRONG` (T6.8), one `ERGONOMIC` (T6.6). Every
entry carries exactly one verdict and no hybrids; the four *worked well* entries are the
protocol's "what worked well" register rather than a verdict, as in T1–T5. No `LANGUAGE-GAP`
and no `DOC-GAP`: everything this component wanted to *be* was expressible, and nothing
required opening Rust, a proposal, or a test — T6.2's limitation has its own headed section on
the shipped website, and T6.8's is the doc being wrong rather than silent. T6.11 was
originally deferred inside T6.8 as an escalation; the controller ruled the language is in
remit, and it is now filed as its own entry.

**The construct is good and the surrounding toolchain is not ready for it.** That split is
sharper here than anywhere else in this log. The body contract is the best-explained
restriction in six tasks — three distinct codes, each carrying its rationale, each enforced
on both legs (T6.1) — and it was *repaired two hours before this task started*, which is
the only reason the brief's Step 3 probe is a pass rather than a false green. The variation
surface is complete on first reach: nesting, param threading, params inside `<match>` arms,
and a `<match>` fold whose three tools agree (T6.5). Provenance is carried on all four
consumer surfaces (T6.9). And the one restriction that blocked a natural beat — no `::set` in
a body — is, on inspection, the right call for a reason the rationale does not even claim:
not purity, but that the number this whole prologue is about would stop being auditable by
reading if eleven files could charge it invisibly (T6.4).

**Then the findings that decide the maturity question.** T6.2: a component's own `uses:`
is discarded, so it has no denotation of its own — two callers took one body and produced
two-command and three-command streams with opposite staging semantics, at exit 0, with no
diagnostic. The docs describe this as "a scoping limit, not a checking divergence"; that
sentence is true about checking and false about meaning, and it is why the verdict is
`SPEC-WRONG` rather than a doc finding. T6.10: adopting the reuse mechanism took a line
*out* of the localization pipeline — `loc export` correctly emits it once, with
`lineId: null`; `loc import` skips it at exit 0 and prescribes `lute tag`, which answers
`already tagged`; `compile --locales` then warns about a caller-derived id that appears in no
export the translator ever saw, and ships English. Verified end to end, with a one-commit
before/after.

**The pattern this log has been accumulating shows up here as a matched pair, and that is the
finding worth carrying forward.** T6.3: the standalone leg can point *into* a component file
and cannot detect a caller-relative fault (it reports `ok`); the caller leg detects every
such fault and cannot point into the file (it reports `1:1`, N times, in the N files that are
correct, while the one file to edit reports `ok`). T6.7 is the same pair on a different fault
— the component's own check blames `@pressure` for a malformed `params:` that the caller's
check names outright. Neither is a missing capability; both are information the same binary
prints from the other leg, one command apart. To be exact about the cost, because the first
draft of this section overstated it: a component **can** be validated — `check-project`
detects the caller-relative fault and exits 1. **What no command offers is a check that is
both caller-context-aware and able to point at the component's own source span**, so the
author is handed N identical `1:1` reports in the N files that are correct while the one file
to edit reports `ok`. Until `lute check <component>` either forwards the caller-side span or
refuses to claim `ok`, the honest instruction to an author is: never check a component file,
only ever `check-project`, and read the path out of the message prefix.

**What a production would need before shipping a translated work with components.** In
order: T6.10 (i) — `loc export` per expansion with caller-derived `lineId`s, without which
components and localization are mutually exclusive; T6.2 (ii) —
`W-COMPONENT-VOCAB-DIVERGENT`, which is nearly free given what `check-project` already
resolves; T6.11 — admit `string` as a renderable type, without which a param cannot vary a
component's words and the construct's headline feature is decorative; T6.3 — forward the
component-internal span and roll up the N caller reports. The first is a blocker. The rest are
the difference between a construct you can use and one you can trust.
