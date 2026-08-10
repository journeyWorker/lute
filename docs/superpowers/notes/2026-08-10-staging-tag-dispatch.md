# The staging reducer dispatches on the source tag, not the record kind — filed for 0.11.0

*Measured 2026-08-10 on `feat/lute-0.10.1` at `422457c` (the `v0.10.0` tag, no code changes).
`lute version` → toolchain `0.10.0`, language `0.10.0`, IR schema `0.10.0`. All repro
documents live under `/tmp/stage-dispatch/`; nothing under this worktree's corpus was
touched or added.*

## What this is

Two defects landed in 0.10.1 out of a real adoption project — an OSHiZ visual-novel
prototype consuming Lute 0.10.0 artifacts. This is the third item from the same project: a
finding, not a fix. It is filed for **0.11.0** with the evidence a design decision needs,
because the obvious-looking patch was already proposed and rejected — twice — and the
rejection's own reasoning is sound. Verdict: **LANGUAGE-GAP** (argued below, against the
`TOOL-DEFECT` candidate).

## The mechanism

`crates/lute-check/src/inject.rs` walks a document's `Node` stream and threads a
`StageState` through it (module doc, `inject.rs:1-58`). Four named rules read that state:
`auto-anchor-on-show`, `entry-emotion-lookahead`, `auto-pose-reset`, and
`stage-bookkeeping`. Dispatch into all four is a literal match on the AST node's own tag:

```rust
// crates/lute-check/src/inject.rs:178-190
match node {
    Node::Directive(d) if d.tag == "auto" => lower_auto(&mut state, d, lookahead, &mut emit, domains),
    Node::Directive(d) if d.tag == "bg" => stage_bookkeeping_bg(&mut state, d, &mut emit),
    Node::Directive(d) if d.tag == "music" => { /* bookkeeping only */ }
    Node::Line(l) => lower_line(&mut state, l, lookahead, &mut emit, domains),
    _ => {}
}
```

Separately, `crates/lute-compile/src/lower.rs` lowers a directive to its **IR command**.
`::bg` hardcodes to `Command::Background` (`lower.rs:152`); a *plugin* directive that
declares `lower: { record: background, fields: {…} }` also produces `Command::Background`,
through a completely different code path (`lower_record`, `lower.rs:320-379`), gated only
on `lute_manifest::validate::lower_record_fields(record)` — the manifest-declared record
kind, not the directive's own tag. **The two lowered records are byte-identical in the
artifact.** The reducer above never sees that path; it only ever sees `d.tag`. So a plugin
directive lowering to `background` produces a `background` record with none of the stage
semantics `inject.rs` attaches to `::bg`.

**One correction to the assignment's framing:** the manifest spelling given —
`lower: { kind: record, record: background, fields: {…} }` — carries an extra `kind: record`
key that is not part of the schema. `Lowering` (`lute-manifest/src/schema.rs:187-196`) is a
`#[serde(untagged)]` enum with exactly two shapes, `Record { record, fields }` and
`Builtin { kind, name }`; there is no `kind: record` discriminant. I tested the literal
spelling from the assignment (`/tmp/stage-dispatch/kindcheck/`) — it parses without error,
because serde's untagged matching for a struct without `deny_unknown_fields` silently drops
the extra key, and the directive still lowers correctly. So the extra key is harmless but
meaningless; the correct, minimal, and only form the codebase itself uses
(`crates/lute-manifest/tests/plugin_validation.rs:229-230`) is
`lower: { record: background, fields: { assetId: { fromAttr: img } } }`. I use the minimal
form below.

## The measurement

Two projects, `/tmp/stage-dispatch/docA` (core `::bg`) and `/tmp/stage-dispatch/docB`
(plugin `::place`), identical scene bodies:

```
# docA/a.lute frontmatter: kind: scene, character: ann, season: 1, episode: 1, uses: schema.yaml
## Room A
::bg{location="room_a"}
::auto{character="ann" action="walk-in"}
@ann{code="0010"}: Hello from room A.
::bg{location="room_b"}
@narrator{code="0020"}: Ann is never shown again.
```

```
# docB/b.lute frontmatter: kind: scene, character: ann, season: 1, episode: 1,
#                           uses: schema.yaml, profile: story
## Room A
::place{location="room_a"}
::auto{character="ann" action="walk-in"}
@ann{code="0010"}: Hello from room A.
::place{location="room_b"}
@narrator{code="0020"}: Ann is never shown again.
```

`docB`'s plugin, `place.pack` (`plugins/place.pack/directives/place.yaml`):

```yaml
directives:
  - name: place
    attrs:
      - { name: location, required: true, type: string }
    lower: { record: background, fields: { location: { fromAttr: location } } }
```

Both check clean and compile clean under `--project`:

```
$ lute check /tmp/stage-dispatch/docA/a.lute --project /tmp/stage-dispatch/docA
ok: /tmp/stage-dispatch/docA/a.lute (0 warning(s))
rc=0
$ lute check /tmp/stage-dispatch/docB/b.lute --project /tmp/stage-dispatch/docB
ok: /tmp/stage-dispatch/docB/b.lute (0 warning(s))
rc=0
$ lute check-project /tmp/stage-dispatch/docA
ok: /tmp/stage-dispatch/docA (1 file(s), 0 project-wide warning(s))
$ lute check-project /tmp/stage-dispatch/docB
ok: /tmp/stage-dispatch/docB (1 file(s), 0 project-wide warning(s))
```

**No diagnostic anywhere distinguishes the two documents.** The consequence is purely an
artifact difference — this is the load-bearing fact behind the design tension below, not a
missed warning:

```
$ lute compile /tmp/stage-dispatch/docA/a.lute --project /tmp/stage-dispatch/docA -o /tmp/stage-dispatch/outA/a.json
$ lute compile /tmp/stage-dispatch/docB/b.lute --project /tmp/stage-dispatch/docB -o /tmp/stage-dispatch/outB/b.json
$ diff <(jq -c '.commands[]' outA/a.json) <(jq -c '.commands[]' outB/b.json)
@@ -2,6 +2,5 @@
 {"kind":"sprite","addr":"001-0200","character":"ann","action":"walk-in"}
 {"kind":"sprite","addr":"001-0300","character":"ann","anchor":"center","provenance":{"injected":true,"by":"auto-anchor-on-show","explanation":"`ann` shown without an explicit anchor; defaulting to `center`"}}
 {"kind":"line","addr":"001-0400","role":"dialogue","speaker":"ann","text":"Hello from room A.","lineId":"ann.s01ep01.ann_0010","voiceKey":"ann-0010"}
-{"kind":"sprite","addr":"001-0500","character":"ann","exit":true,"provenance":{"injected":true,"by":"stage-bookkeeping","explanation":"auto-hiding `ann` left on stage across a scene change"}}
-{"kind":"background","addr":"001-0600","location":"room_b","wait":true}
-{"kind":"line","addr":"001-0700","role":"narration","speaker":"narrator","text":"Ann is never shown again.","lineId":"ann.s01ep01.narrator_0020"}
+{"kind":"background","addr":"001-0500","location":"room_b","wait":true}
+{"kind":"line","addr":"001-0600","role":"narration","speaker":"narrator","text":"Ann is never shown again.","lineId":"ann.s01ep01.narrator_0020"}
```

`docA` injects exactly the command the assignment predicted:
`{"kind":"sprite","addr":"001-0500","character":"ann","exit":true,"provenance":{"injected":true,"by":"stage-bookkeeping","explanation":"auto-hiding \`ann\` left on stage across a scene change"}}`.
`docB` injects **nothing** at the scene change — the `::place` node's tag is `"place"`, never
matches `d.tag == "bg"` on `inject.rs:182`, so `stage_bookkeeping_bg` never runs, `ann`'s
entry stays in `StageState.on_stage`, and no `Hide` command is ever built. An engine that
renders the artifact literally (no independent stage-clearing of its own — which the
adjacent gap below shows is a live possibility, not a strawman) leaves `ann` visible in
`room_b`. This is exactly the claimed defect, confirmed byte-for-byte.

## `W-STAGE-ABSENT`, `auto_pose_reset`, `entry_emotion_lookahead`: verified, but not the way the assignment assumed

I checked each named rule rather than assuming it diverges "the same way" as the sprite-exit
above. Two of the three do not — and the actual mechanism is more interesting, and in one
case *worse*, than "B silently drops what A has."

### `W-STAGE-ABSENT` — real divergence, but keyed on a different state field

`W_STAGE_ABSENT` fires only from `StageState.exited` — the set populated **exclusively** by
an *explicit* `::auto{action=<exit-domain-member>}` (`inject.rs:219-228`), never by the
implicit scene-change auto-hide. And `stage_bookkeeping_bg` (the `::bg`-tag-matched rule)
**clears** `exited` on every scene change (`inject.rs:434`, comment: *"a `::bg` is a scene
change: every sprite is auto-hidden above, so no earlier exit constrains what follows"*).
So the naive test — add a second line for `ann` after the scene change in `docA`/`docB`
above, no re-`::auto` — fires nothing in either document (verified: both `check-project`
runs stay `ok`, 0 warnings). Presence-without-a-show is not what this rule checks; an
explicit, unresolved *exit* is.

That is exactly the shape of the field evidence's phone-call/text-exchange gaps. Constructed
directly — `ann` explicitly exits in scene 1, the scene changes, `ann` speaks again in scene
2 with no re-show:

```
$ lute check-project /tmp/stage-dispatch/docA3   # core ::bg
ok: /tmp/stage-dispatch/docA3/a3.lute (0 warning(s))
ok: /tmp/stage-dispatch/docA3 (1 file(s), 0 project-wide warning(s))

$ lute check-project /tmp/stage-dispatch/docB3   # plugin ::place, same shape
/tmp/stage-dispatch/docB3/b3.lute:23:1: warning [W-STAGE-ABSENT] `ann` left the stage on an
earlier declared exit and has not been shown again, so a spoken line here stages someone who
is not present. Show them again with an `::auto` before this point, or remove the earlier
exit (dsl 0.10.0 §11.2)
ok: /tmp/stage-dispatch/docB3/b3.lute (1 warning(s))
```

`docA` is silent because `::bg`'s tag-matched auto-hide clears the earlier exit's memory —
"a scene changed, start over." `docB` warns because `::place` never triggers that clear, so
the stale `exited` entry from scene 1 survives into scene 2 and correctly (if surprisingly,
from the perspective of someone who only ever tested against `::bg`) fires. **This is the
mechanism, positive-controlled, behind the field evidence's "core `::bg` was silently
papering over two genuine gaps."** It is not `stage-bookkeeping`'s *auto-hide* diverging —
it is `stage-bookkeeping`'s *unconditional reset of `exited`* diverging, since that reset is
tag-gated exactly like the auto-hide is.

Positive control that the rule itself is not plugin-specific or otherwise broken — two
explicit exits back to back under *pure* `lute.core`, no plugin at all:

```
$ lute check-project /tmp/stage-dispatch/docPC
/tmp/stage-dispatch/docPC/pc.lute:17:1: warning [W-STAGE-ABSENT] `ann` left the stage on an
earlier declared exit and has not been shown again, so another declared exit here stages
someone who is not present. …
```

### `entry_emotion_lookahead` and `auto_anchor_on_show` — the tag mismatch causes a MISCLASSIFICATION, not a missing injection

Both rules run only on the **entrance** branch of `lower_auto` — reached when the character
is *not already* in `StageState.on_stage` (`inject.rs:233-253`). Because `place` never
clears `on_stage` at a scene change, a character who re-enters a *later* scene via a fresh
`::auto` in a `::place`-based document is not treated as entering at all — she is still
"on stage" from the *previous* scene, so `lower_auto` takes the *reposition* branch
(`inject.rs:233-247`) instead, which runs neither rule. Two full scenes, `ann` walks in at
the top of each:

```
$ lute compile … docA4   # core ::bg — SECOND ::auto correctly treated as an entrance
… scene 2 …
{"kind":"sprite","addr":"002-0300","character":"ann","anchor":"center","provenance":{"injected":true,"by":"auto-anchor-on-show", …}}
{"kind":"sprite","addr":"002-0500","character":"ann","preload":true,"emotion":"surprised","provenance":{"injected":true,"by":"entry-emotion-lookahead", …}}
{"kind":"line","addr":"002-0600", … "emotion":"surprised", …}

$ lute compile … docB4   # plugin ::place — SECOND ::auto misread as a reposition, not an entrance
… scene 2 …
{"kind":"sprite","addr":"002-0200","character":"ann","action":"walk-in"}
{"kind":"line","addr":"002-0300", … "emotion":"surprised", …}
```

`docB4`'s scene 2 gets neither the anchor default nor the emotion preload — the artifact has
`ann` speaking `surprised` with no sprite pre-loaded at that emotion, and no anchor set for
her *at all* in that scene (her stale `anchor: "center"` from scene 1's now-inapplicable
state is the only value that ever existed). This is a missing injection, as claimed, but the
*cause* is not "the entrance rule didn't run for a plugin tag" — it is "the reducer thinks
this was never an exit, so it isn't an entrance either."

### `auto_pose_reset` — the tag mismatch causes a SPURIOUS injection, the opposite direction

`auto_pose_reset` fires when `!stateful && dirty.contains(speaker) && on_stage.contains(speaker)`
(`inject.rs:390`). `stage_bookkeeping_bg` clears both `dirty` and `on_stage` on a real `::bg`;
`::place` clears neither. Scene 1: `ann` speaks a stateful (`emotion=`) line, marking her
dirty. Scene changes. Scene 2: `ann` speaks a **plain** line, no re-`::auto` at all:

```
$ lute compile … docA5   # core ::bg
… scene 2 …
{"kind":"background","addr":"002-0200","location":"room_b","wait":true}
{"kind":"line","addr":"002-0300", …}                                    ← no posReset; correct, she's off stage

$ lute compile … docB5   # plugin ::place
… scene 2 …
{"kind":"background","addr":"002-0100","location":"room_b","wait":true}
{"kind":"sprite","addr":"002-0200","character":"ann","posReset":true,"provenance":{"injected":true,"by":"auto-pose-reset","explanation":"`ann` had a dirty pose before a plain line; resetting to neutral"}}
{"kind":"line","addr":"002-0300", …}
```

`docB5` injects a `posReset` sprite command **targeting a character the artifact never
placed in that scene** — the reducer believes `ann` is still on stage from scene 1 (nothing
tag-matched cleared her) and dirty (nothing tag-matched cleared that either), so it "resets"
a sprite that, per the artifact's own record stream, does not exist in `room_b`. This is the
opposite failure from the entrance case above: there the plugin path silently drops a
command; here it silently fabricates one. Both are downstream of the same cause — `on_stage`
and `dirty` are never cleared because nothing in `inject.rs` matches `d.tag == "place"` —
but "diverge the same way" undersells it. The three rules diverge through **three different
symptoms** off two different pieces of tag-gated state (`exited` vs. `on_stage`/`dirty`), not
one repeated shape.

## The field evidence

The adoption project migrated 7 scenes from core `::bg` to a plugin `::place` lowering to
`background`, for compile-time `assetId` validation. That migration is reported to have
surfaced three `W-STAGE-ABSENT` warnings: one already-known authoring defect, and two
genuine gaps — a phone call and a text exchange, both staging a character who was narratively
absent — that `::bg`'s scene-change auto-hide had been silently absorbing by resetting
`exited` (and `on_stage`) at every background change, exactly as the `docA3`/`docB3` positive
control above demonstrates the mechanism doing. **I did not have access to that external
project and cannot re-verify the count of three**; what I verified directly is that the
mechanism the report describes is real, reproducible, and produces exactly the asymmetry
claimed — `::bg` silences a real defect by resetting `exited` on every scene boundary,
`::place` (or any other record-`background`-lowering plugin directive) does not, and so
correctly surfaces it. The report's framing — *"the current behaviour is not merely
inconsistent; in one direction it hides real authoring defects, in the other it drops
injected commands"* — is accurate to what I measured, and *"drops"* is now known to
undersell the `entry_emotion_lookahead`/`auto_pose_reset` half: one direction hides real
defects, the other direction both drops correct injections and fabricates incorrect ones,
depending which of the four rules is asked.

## The design tension — quoted, not asserted

The obvious "just branch on a flag" fix was proposed and **rejected twice**, independently,
for the identical reason:

> "Driving reducer dispatch from `semantics` flags. The reducer keeps matching on the
> directive tag. `mutatesScene` is declared on both `::bg` and `::music`, so branching on it
> would make `::music` clear the stage."
> — `docs/proposals/scenario-dsl/0.9.0.md:317-319`, §7 Non-goals

> "Wiring the surviving flags to drive reducer dispatch remains a **non-goal** (dsl 0.9.0
> §7). `mutatesScene` is declared on both `::bg` and `::music`, so branching on it would make
> `::music` clear the stage; the reducer keeps matching on the directive tag."
> — `docs/proposals/plugin-system/0.0.3.md:160-163`, §4

Both citations are correct as given (the assignment's `159-163` is the passage; the sentence
itself sits at `160-163`). Read against the current core flags
(`crates/lute-manifest/assets/lute.core/directives/staging.yaml`): `bg` and `music` both
carry `semantics: ["mutatesScene"]`; nothing separates "clears the stage" from "changes
scene-level audio." A reducer that branched on `mutatesScene` alone would make `::music`
auto-hide every sprite — wrong for `::music`, and it would still say nothing about `::place`,
since `::place` declares no `semantics:` at all in my repro (a plugin directive that merely
lowers to `background` is not obligated to declare anything about staging). **The set as it
exists genuinely cannot drive this dispatch**, and the rejection is correctly reasoned, not
merely under-implemented.

Against that, `crates/lute-check/src/inject.rs:47-58`'s own module doc calls exactly this
swap a **mechanical follow-up**:

> "At Task 4.8 the reducer hardcodes the *known* `lute.core` staging vocabulary (`::auto` =
> entrance/exit/pose, `::bg` = scene change, `::line` emotion/pose attrs) rather than reading
> those flags, because a stable, documented baseline is more valuable here than a premature
> flag-driven dispatch. Swapping the `is_*`/tag checks below for `semantics`-flag lookups is
> a mechanical follow-up once the resolver consumes a `CapabilitySnapshot`."

**These two statements are in tension**, and that tension — not an oversight — is why this
isn't a 0.10.1 patch. `inject.rs` calls the swap mechanical, as if the only missing
ingredient were a `CapabilitySnapshot` the resolver already threads through by 0.10.0. The
proposal series calls the *identical* swap a considered non-goal, twice, for a reason that
holds against the *current* flag set. Both can be true at once only if "mechanical" was
written against an *imagined future* flag set that does not exist yet — which is exactly
what 0.11.0 has to decide, not discover it already has.

### What that leaves to design, named without deciding

Given `bg`/`music` both carry `mutatesScene` and `auto` carries `["reads.onStage",
"usesAnchor", "mayExitCharacter", "writes.characterState"]`
(`crates/lute-manifest/assets/lute.core/directives/staging.yaml`), neither the current flag
set nor a plugin directive's tag is sufficient to say "this directive clears the stage the
way `::bg` does." Two shapes, not decided here:

1. **A new closed flag, e.g. `clearsStage`**, declared on `::bg` and by any plugin directive
   whose lowered record should carry the same scene-change semantics. Cost: the ten-flag
   `semantics` vocabulary (`docs/proposals/plugin-system/0.0.3.md` §4) is closed and
   core-owned by design — *"the set remains owned by the core: a plugin MUST NOT invent flag
   names"* — so adding an eleventh flag needs a plugin-system spec delta, not just a code
   change.
2. **Making the lowered record kind carry the meaning intrinsically** — a `background`
   record always clears the stage regardless of which directive tag produced it, so
   `stage_bookkeeping_bg` dispatches on `LOWER_RECORDS`'s `"background"` kind (which
   `inject.rs` does not currently read at all) rather than `d.tag == "bg"`. Cost: this
   changes behavior for every currently-passing plugin directive that lowers to
   `background`/`sprite`/etc. without expecting stage-clearing side effects — a
   currently-green document's *artifact* changes with no `LANG`-axis grammar change, which
   is exactly what `dsl 0.10.0`'s own boundary calls out as needing a delta anyway.

Either way, `scenario-dsl/0.10.0.md`'s own scoping test decides the axis:

> "Does this change **what a document is allowed to be**, or **what the checker emits about a
> legal one**?"
> — `docs/proposals/scenario-dsl/0.10.0.md:80-81`, §2.1

Both shapes change what the checker emits about a legal, currently-`ok` document (whether
`W-STAGE-ABSENT` fires, whether a `background`-lowering plugin directive's document gets an
injected `exit` record it did not get in 0.10.1) — so by the test's own first half, this is
language-axis and needs a `scenario-dsl` delta alongside whichever plugin-system delta
carries the mechanism. 0.11.0 has to pick a shape and write both.

## Also record, separately: the sentence that currently makes none of this a spec violation

> "A conforming document MUST reduce, at build time, to a finite ordered sequence of command
> records + CEL strings (invariant §3.2). Implicit staging that an implementation injects
> (stage hygiene) is **implementation-defined** and outside this language spec; it MUST be
> deterministic and MUST NOT change the meaning of explicitly authored constructs."
> — `docs/proposals/scenario-dsl/0.0.1.md:518-521`, §11.5 Reduction

Confirmed unchanged through `0.1.0.md:874-877` and referenced (not amended) at `0.4.0.md:597`;
`0.10.0.md` does not mention §11.5 at all — grepped for `implementation-defined` and `§11.5`
across it, zero hits. This sentence is why the `docA`/`docB` measurement above is not a spec
violation today: stage hygiene is explicitly out of the language's own jurisdiction, so
`::bg` clearing the stage and `::place` not clearing it are both, individually, conforming —
each is a deterministic function of its own implementation. It is also why two toolchain
surfaces (a checker warning keyed on `exited`, an engine that trusts the artifact literally)
can disagree with each other without either one being *wrong* against anything written down.

**If 0.11.0 makes staging plugin-aware — either flag or record-kind shape above — this
sentence probably has to move.** A `clearsStage` flag or a record-kind rule that a plugin
author can rely on is no longer "whatever this implementation happens to do"; it is a
cross-plugin contract two different capability packages both depend on, which is the
opposite of implementation-defined. Not deciding this now; naming it so 0.11.0 does not
silently orphan §11.5 while fixing the reducer underneath it.

## Verdict: `LANGUAGE-GAP`, not `TOOL-DEFECT` — argued

The case for `TOOL-DEFECT`: the checker treats two IR-identical `background` records
differently depending on which surface syntax produced them, inside code that already
exists and already claims (in its own module doc) that the fix is "mechanical." An internal
inconsistency in already-shipped logic is the textbook shape of a tool defect, and the
`auto_pose_reset` spurious-injection case above is a genuine bug by any reading — it fabricates
a command that misrepresents the scene.

The case for `LANGUAGE-GAP`, which I land on: `TOOL-DEFECT` in this log's vocabulary means the
implementation diverges from an established contract. There is no established contract here
to diverge from. `scenario-dsl/0.0.1 §11.5` explicitly and by design leaves stage hygiene
**implementation-defined** — the reduction invariant does not promise `::place` behaves like
`::bg`, and never has. `scenario-dsl/0.9.0 §7` and `plugin-system/0.0.3 §4` each *considered*
tying dispatch to declared semantics and *declined*, correctly, against the flag set as it
stands — this is a decision the project made on purpose, twice, not a corner nobody got to.
What's missing is not an implementation matching a spec; it's a spec position on a question
the language has never actually answered: whether a plugin-lowered record should inherit the
core directive's staging semantics, and if so, through which mechanism. `inject.rs`'s own
"mechanical follow-up" framing is itself evidence the boundary was never settled — that
comment describes a swap against a flag vocabulary that doesn't yet carry the needed
distinction, written as though it already did. Fixing the code without first answering that
question produces exactly what the flag-driven-dispatch non-goal warned against: a plausible
mechanism (`clearsStage`, or record-kind dispatch) that is a **guess** about what plugin
authors should be able to rely on, not a spec-derived rule. That is a gap in the language's
account of itself, filed for the release track that writes specs, not the one that ships
patches.

## Adjacent, same-session find: `lower:`'s own grammar is written closed and parses open

Not the subject of this note — filed here because it surfaced while confirming the
`kind: record` correction above, and it is the same shape of defect `0.10.0` closed twice
(`E-UNKNOWN-ATTR` on the logic tags, `E-TEST-KEY` on both nesting levels of `*.test.yaml`).
Its own verdict, kept short.

**The grammar is normative and closed.** `docs/proposals/plugin-system/0.0.1.md:468-471`, §8.2:

> ```
> lower ::= { record: <recordType>, fields: { ... } }   (* declarative: one fixed record *)
>         | { kind: "builtin", name: <hookName> }        (* a narrow, named core hook *)
> ```

Two productions, nothing else — the same closed-BNF convention this project uses elsewhere
(`scenario-dsl/0.10.0.md` §3.1 on `AssignOp`: *"there is no fifth"*). **The implementation
does not enforce it.** `Lowering` (`crates/lute-manifest/src/schema.rs:187-196`) is
`#[serde(untagged)]` with no `deny_unknown_fields`, so a value satisfying one production's
required fields, PLUS any extra garbage keys, is accepted — already shown above:
`{ kind: record, record: background, fields: {…} }` parses as `Record`, silently dropping
`kind`. This is a real closed-key-set hole, not a cosmetic one: a plugin author who
typos toward the *other* variant's key gets no diagnostic at all, and `kind:` is exactly the
typo `Builtin`'s own grammar invites, since `Builtin` really does take a `kind:` field.

**Verified before writing this up**, per the request not to assert a fix's cost without
checking it:

- **No struct in `lute-manifest` carries `#[serde(deny_unknown_fields)]` anywhere** (grepped
  `crates/lute-manifest/src/*.rs`, zero hits) — this is not a one-off gap in `Lowering`, it is
  the crate's uniform posture: every manifest struct silently accepts unknown keys. `Lowering`
  is simply the one case where the accepted shape is ALSO closed by an explicit written
  grammar, which is why it is the one worth filing.
- **`#[serde(deny_unknown_fields)]` on an individual variant of an untagged enum does not
  compile.** I tried it first, since it looked like the natural fix, and serde's derive macro
  rejects it outright:
  ```
  error: unknown serde variant attribute `deny_unknown_fields`
  ```
  `deny_unknown_fields` is a container-level attribute only; there is no way to close one
  variant's key set while leaving a sibling variant open, inside one untagged enum.
- **`#[serde(untagged, deny_unknown_fields)]` on the whole enum compiles, and closes exactly
  this hole**, verified with a standalone `serde`/`serde_yaml` crate pinned to this
  workspace's versions (`serde = "1"`, `serde_yaml = "0.9"`), reproducing `Lowering` and its
  containing `DirectiveDecl`/`DirectivesFile` shapes exactly:
  ```
  === nested in DirectivesFile, deny_unknown_fields on Lowering only ===
  ERR: directives[0]: data did not match any variant of untagged enum Lowering at line 2 column 5
  === legit record form still parses ===
  OK: DirectivesFile { directives: [DirectiveDecl { … lower: Record { record: "background", … } }] }
  === legit builtin form still parses ===
  OK: DirectivesFile { directives: [DirectiveDecl { … lower: Builtin { kind: "builtin", … } }] }
  ```
  Both legitimate forms are unaffected. I also grepped every `lower:` declaration in this
  repository — `docs/`, `crates/lute-manifest/`, every proposal and worked example — and none
  uses a third key or a shape wider than the two productions above, so this closure would not
  break anything currently shipped (not a claim I can extend to a hypothetical third-party
  manifest, which is exactly the population this diagnostic protects).
- **The resulting error message is the SAME one already shown live through the real `lute`
  binary for a `lower:` block that satisfies neither production today** — this is not a new
  diagnostic surface, it is the untagged enum falling through to a failure path that already
  exists and is already wired to `E-PLUGIN-PARSE`:
  ```
  $ lute check /tmp/stage-dispatch/parsecheck/k.lute --project /tmp/stage-dispatch/parsecheck
  lute: E-PLUGIN-PARSE: Parse { file: "…/directives/bad.yaml", msg: "directives[0]: data did
  not match any variant of untagged enum Lowering at line 2 column 5" }
  ```
  (`lower: { foo: bar }` — a `lower:` satisfying neither shape at all.) Note this is already
  the *worst-quality* diagnostic surface in the manifest pipeline: `LoadError`'s `Debug`
  formatting is printed verbatim (`Parse { file: …, msg: … }`, not a formatted `error [CODE]
  …` line), unlike every hand-rolled `ManifestError`/checker diagnostic elsewhere in this
  codebase.

**What the fix costs, honestly.** Wiring `deny_unknown_fields` onto the enum is a one-line,
verified-safe change that closes the hole — but it does not raise the message quality at all;
an author gets the same generic *"data did not match any variant"* text with no offending key
named and no did-you-mean, whether they wrote one garbage key or ten. Matching the quality bar
`E-UNKNOWN-ATTR`/`E-TEST-KEY` set (name the bad key, suggest the closest legal one, say which
variant was probably intended) needs either a hand-written `Deserialize` impl for `Lowering`
or a two-phase load (deserialize into `serde_yaml::Mapping` first, classify by which required
keys are present, then report the specific extra/missing key) — real, un-mechanical work, not
a one-liner. Whichever is chosen, it belongs in `lute-manifest`'s loader/schema layer, not in
`validate.rs`: `validate_record_lowering` (`crates/lute-manifest/src/validate.rs:258-360`)
only ever sees the ALREADY-DESERIALIZED `Lowering::Record { record, fields }` — by the time it
runs, an extra top-level key is already gone and there is nothing left for it to report.

**One more instance of the same root cause, noted but not chased down:** `Lowering::Builtin`'s
own `kind` field is a bare `String`, never compared to the literal `"builtin"` the grammar
names — every read of it (`lower.rs:262`, `snapshot.rs:345-349`, `validate.rs:563-567`, 3 more
call sites) matches the variant with `{ .. }` or constructs it directly, never checks the
value. `lower: { kind: "record", name: "background" }` (no `record:`/`fields:` at all)
deserializes as a valid `Builtin` lowering to a hook named `background` — reproduced live via
the real CLI, `ok: … (0 warning(s))` — even though `"builtin"` is the only literal §8.2's
grammar admits there, and `docs/proposals/plugin-system/0.0.1.md:477` separately states
`name` "MUST resolve to a registered hook," which I found no validation of anywhere in
`lute-manifest` or `lute-compile`. That is its own gap, arguably larger than the one this
entry is about, and out of scope for a short adjacent note — flagging it so it is not lost.

**Verdict: `TOOL-DEFECT`.** Unlike the main entry above, there is a written, normative,
closed grammar for `lower:` (`plugin-system/0.0.1.md` §8.2) that the implementation simply
does not enforce — this is not a deferred design question, it is an unenforced existing rule,
the textbook shape of every other `TOOL-DEFECT` in this project's vocabulary (`E-UNKNOWN-ATTR`
closing the logic-tag attribute surfaces, `E-TEST-KEY` closing `*.test.yaml`'s keys — both
`TOOL-DEFECT` in `2026-08-06-anseo-drive-test-0.10.0.md`). The fix is well-understood and
partially free (the enum-level attribute) but not entirely free (message quality needs real
work) — filed rather than fixed, per the same instruction governing the rest of this note.

## Corrections to my own reproduction of the assignment's framing

1. **The manifest spelling.** `lower: { kind: record, record: background, fields: {…} }` (as
   given) parses only because the extra `kind: record` key is silently ignored — `Lowering`
   has no `kind: record` discriminant. Not a defect (it costs nothing and does not mislead
   the compiler), but worth fixing in any future citation: the correct form is
   `lower: { record: background, fields: {…} }`, matching
   `crates/lute-manifest/tests/plugin_validation.rs:229-230` and the repro above.
2. **`::auto{character="ann" action="enter"}`** is not a legal construction against any
   schema shipped in this repo — no `action` domain member in `docs/examples/base.schema.yaml`
   or `docs/examples/anseo/vocabulary.schema.yaml` is named `enter`. I substituted a real
   member (`walk-in`) from a minimal schema I declared for the repro; this does not affect
   any claim above, all of which turn on the `bg`/`place` tag, not the entrance action's name.
3. **"`W-STAGE-ABSENT`, `auto_pose_reset` and `entry_emotion_lookahead` diverge the same
   way"** does not hold literally. All three diverge *because of* the same root cause (tag
   dispatch never running `stage_bookkeeping_bg` for a plugin-lowered `background` record),
   but through two different pieces of state (`exited` vs. `on_stage`/`dirty`) and three
   different symptoms: a warning that moves from silent to firing (`W-STAGE-ABSENT`), an
   injection that goes missing (`entry_emotion_lookahead`/`auto_anchor_on_show`), and an
   injection that gets fabricated for a character no longer in the scene
   (`auto_pose_reset`). The write-up above documents each shape rather than asserting the
   analogy holds.
4. **The field evidence's "surfaced three `W-STAGE-ABSENT` reports"** is reported to me, not
   independently re-derived — I do not have the OSHiZ adoption project's repository in this
   worktree. What I verified is the *mechanism* the report attributes the finding to
   (`docA3`/`docB3`/`docPC` above), which reproduces exactly and supports the report's
   reading. The specific count of three is taken on the assignment's word.

## Reproduction inventory

All under `/tmp/stage-dispatch/`, none added to the corpus:

| dir | purpose |
| --- | --- |
| `docA` / `docB` | primary measurement — identical scene, core `::bg` vs. plugin `::place` |
| `docA2` / `docB2` | a bare re-speak after scene change with no prior exit — both silent (establishes `W-STAGE-ABSENT` is not keyed on mere absence) |
| `docA3` / `docB3` | explicit exit, then scene change, then a re-speak — the field-evidence phone-call shape |
| `docA4` / `docB4` | two full scenes, second `::auto` entrance — `entry_emotion_lookahead`/`auto_anchor_on_show` |
| `docA5` / `docB5` | stateful line, scene change, plain line — `auto_pose_reset` |
| `docPC` | positive control: `W-STAGE-ABSENT` fires under pure `lute.core`, no plugin involved |
| `kindcheck` | the assignment's literal `kind: record` spelling, confirmed to parse (silently ignored extra key) |
