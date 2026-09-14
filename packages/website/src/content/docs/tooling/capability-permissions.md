---
title: Capability permissions
sidebar:
  label: Capability permissions
description: Restrict authored directives, state and fact writes, bridges, rewards, and quests with project profiles or a trusted host permission profile—without mistaking compile-time admission for a runtime sandbox.
---

Capability permissions let a trusted project or host place a ceiling over what
a `.lute` document may author. The checker rejects forbidden effects before the
compiler emits an artifact. The policy is generic and independent of whichever
engine consumes Lute IR.

This is an **Unreleased toolchain feature**, not a published-version claim. The
[normative plugin-system delta](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.6.md)
and [runtime/security guide](https://github.com/journeyWorker/lute/blob/main/docs/runtime/capability-permissions.md)
are the source contracts.

## Configure authored and restricted profiles

```yaml
# lute.project.yaml
defaultProfile: authored

# Root permissions are absent, so there is no project-wide ceiling.
profiles:
  authored:
    plugins: {}
    # permissions is absent: unrestricted
  restricted:
    plugins: {}
    permissions:
      directives: [camera, bg, music, sfx, end, set, use]
      stateWrites: [scene.*]
      factWrites: []
      bridges: []
      rewards: false
      quests: false
```

The complete project-file shape is published as
[`schemas/lute.project.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute.project.json).

The fields are:

| Field | Entries | Match |
| --- | --- | --- |
| `directives` | directive names without `::`, or `*` | exact |
| `stateWrites` | dot paths, `*`, or a suffix `.*` | exact or descendants only |
| `factWrites` | relation names or `*` | exact |
| `bridges` | `service/operation` pairs or `*` | exact |
| `rewards` | boolean | `false` denies; `true` adds no restriction |
| `quests` | boolean | `false` denies; `true` adds no restriction |

**Absent and empty are different.** An absent field imposes no additional
restriction. `factWrites: []` explicitly permits no fact write. Likewise,
`rewards: true` is unrestricted while `rewards: false` denies rewards. Explicit
`null`, unknown fields, malformed patterns, and wrong value shapes are
configuration errors—not permissive fallbacks.

`scene.dialogue.*` admits `scene.dialogue.current` but not the root
`scene.dialogue` or the lookalike `scene.dialogueOther.current`. There are no
regexes, interior wildcards, whitespace patterns, or partial-segment
wildcards. A bridge is always the unambiguous `service/operation` spelling.

## Ceilings only narrow

The project root, reserved `global` profile, every ancestor profile, and the
selected profile apply conjunctively. A field matches any entry inside one
layer, but every layer that restricts that field must admit the operation. A
child cannot widen its parent. Inline plugin activation cannot widen any policy.
Permissions also do not override ordinary language rules such as read-only
`app.*` state.

Permissions never activate plugins or add vocabulary. The normal source profile
and plugin graph resolve first; the effective permission layers then narrow that
snapshot.

## Pin a trusted host ceiling

A source-authored `profile:` is capability selection, not authorization. A host
that accepts authored or generated Lute should choose its ceiling independently:

```console
$ lute check scene.lute --project . --permission-profile restricted
$ lute compile scene.lute --project . --permission-profile restricted -o scene.json
$ lute compile --all --project . --permission-profile restricted -o build
$ lute context scene.lute --json --project . --permission-profile restricted
```

`--permission-profile NAME` applies that profile's project/global/ancestor/name
permissions as an **additional ceiling**. It does not activate the named
profile's plugins and does not rewrite the source profile. A document containing
`profile: authored` therefore cannot widen a host-pinned `restricted` policy.
A missing project or profile is an explicit resolver error.

`compile --all` checks every document with the same host ceiling before writing;
if any document is denied, no denied or partial output set is produced.

For streaming, the same frozen ceiling checks the trusted template and every
body unit:

```console
$ printf '::set{scene.score = 1}\n' \
    | lute compile-stream scenes/live.lute --project . \
        --permission-profile restricted
```

A denied unit yields a terminal NDJSON `error` record and no forbidden update or
`finish`. There is no mid-stream permission switch.

## What gets rejected

The shared checker pass covers more than visible `::name` leaves:

- directives, including `set`, `assert`, `retract`, and component `use`;
- explicit state/fact writes plus choice `into`, implicit choice selection, and
  hub-visit writes;
- state defaults and seed facts, because initialization is an effect;
- plugin-declared state writes and bridge service/operation calls;
- quests, quest lifecycle/objective initialization, and declarative rewards;
- nested branch, match, hub, objective, `on`, and timeline bodies; and
- invoked component bodies under the caller's policy.

Authorization is about admitted source, so a dead guard does not hide a
forbidden reward or effect. A state declaration **without a default** may remain
visible as host-supplied, read-only context. If a restrictive plugin write path
cannot be resolved from its attributes, checking fails closed.

Denied source reports a non-suppressible error at its authored span:

| Code | Category |
| --- | --- |
| `E-PERMISSION-DIRECTIVE` | directive |
| `E-PERMISSION-STATE` | state write/default |
| `E-PERMISSION-FACT` | fact write/seed fact |
| `E-PERMISSION-BRIDGE` | bridge |
| `E-PERMISSION-REWARD` | reward declaration |
| `E-PERMISSION-QUEST` | quest declaration |

`compile` repeats the same permission gate before lowering, so a successful
check result created under another policy cannot be used to smuggle forbidden
IR into an artifact.

## Context and editor behavior

`lute context --json` makes the restriction explicit in four additive fields:

```json
{
  "permissions": {
    "layers": [
      {
        "directives": ["end", "set"],
        "stateWrites": [],
        "factWrites": [],
        "bridges": [],
        "rewards": false,
        "quests": false
      }
    ]
  },
  "bridges": [],
  "rewardKinds": {},
  "questsAllowed": false
}
```

`permissions` is the serialized effective layer set. `bridges` contains only
allowed bridge capability objects, `rewardKinds` contains only allowed
name-keyed reward kinds (empty when rewards are denied), and `questsAllowed`
states whether quest authoring is admitted. The existing `directives` array
also removes a bridge directive when either its directive name or its
`service/operation` is denied.

Context keeps external read-only state visible and does not pretend to enumerate
attribute-dependent plugin write paths. Text output calls the policy a
compile-time authoring restriction—not a runtime sandbox.

The LSP shares the resolver and checker, publishes the same diagnostics, and
removes prohibited directives and bridges from completion. Editor filtering is
a writing aid; `check` and `compile` remain the enforcement boundary.

Restrictive permissions participate in `capabilityVersion`. A wholly
unrestricted policy is normalized away, preserving policy-free capability
hashes byte-for-byte. The hash distinguishes authoring surfaces for caches; it
is not a signature or proof that an artifact was authorized.

## Complete generic example

The repository's
[`docs/examples/capability-permissions/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/capability-permissions)
project contains:

- an `authored` profile with permissions absent;
- a `restricted` profile with explicit empty sets and `false` gates;
- an authored quest whose declarative reward compiles under the ordinary profile
  but is rejected by the host pin; and
- a streaming scene whose no-default state is legal until a generated `::set`
  attempts a denied write.

The example makes no remote or AI call. Its reward remains declarative artifact
data; no settlement is implemented.

## Security non-goals

Capability permissions are compile-time admission control. They do **not**:

- make an AI call or add AI-specific syntax;
- ship an STAGE or other product plugin;
- sandbox the runtime, process, network, filesystem, bridge implementation, or
  secrets;
- implement, roll, grant, settle, or persist rewards; or
- make untrusted plugins, projects, artifacts, or externally supplied
  `capabilityVersion` hashes safe.

The host still authorizes runtime principals and resources, maps bridges only to
intended implementations, owns persistence/idempotency, and loads only trusted
artifacts.
